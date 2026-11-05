package local.doubao.voiceremote

import android.Manifest
import android.app.Activity
import android.content.pm.PackageManager
import android.os.Handler
import android.os.Looper
import org.json.JSONObject
import java.util.UUID
import java.util.concurrent.Executors

/** All session decisions live on the UI thread; microphone and TLS work stay off it. */
class RemoteController(private val activity: Activity, private val changed: () -> Unit) {
    enum class Phase { IDLE, PREPARING, PRIMING, STREAMING, DRAINING }
    var computer: Computer? = null; private set
    var status = "扫描电脑配置页的配对二维码。"; private set
    var phase = Phase.IDLE; private set
    var holding = false; private set
    var level = 0.0; private set
    var pairing = false; private set
    private val main = Handler(Looper.getMainLooper())
    private val io = Executors.newSingleThreadExecutor()
    private val store = PairStore(activity)
    private var active = false
    private var destroyed = false
    private var generation = 0
    private var pressSequence = 0
    private var qualified = false
    private var resumeAfterDrain = false
    private var microphone: Microphone? = null
    private var link: SocketLink? = null
    private var connected = false
    private var enterSupported = false
    private var session: UUID? = null
    private var pendingEnter: UUID? = null
    private var pairingClient: PairingClient? = null

    init {
        try { computer = store.load(); if (computer != null) status = "按住说话即可连接电脑。" }
        catch (_: Exception) { status = "无法读取配对记录，请重新扫码连接电脑。" }
    }
    private fun update() { if (!destroyed) changed() }
    private fun ui(block: () -> Unit) { main.post { if (!destroyed) block() } }
    fun foreground(value: Boolean) { active = value; if (!value) stop("本次输入已结束，返回后请重新按住。") }
    fun permissionResult(granted: Boolean) { status = if (granted) "权限已允许，请重新按住说话。" else "请在系统设置中允许麦克风权限。"; update() }
    fun pair(text: String) {
        stop("正在解析配对二维码。")
        val payload = try { PairPayload.parse(text) } catch (e: Exception) { status = e.message ?: "二维码无法识别。"; update(); return }
        val attempt = generation
        pairing = true; status = "正在连接电脑并完成配对。"; update()
        io.execute {
            var client: PairingClient? = null
            try {
                client = PairingClient(payload.computer)
                val current = client
                ui { if (attempt == generation) pairingClient = current else current.close() }
                val result = current.pair(payload.ticket)
                ui {
                    if (attempt != generation) return@ui
                    try { store.save(result); computer = result; status = "配对成功，按住说话即可连接。" }
                    catch (_: Exception) { status = "无法保存配对，请解锁设备后重新扫码。" }
                    pairing = false; pairingClient = null; update()
                }
            } catch (e: Exception) {
                ui {
                    if (attempt == generation) {
                        pairing = false; pairingClient = null
                        status = if (e is javax.net.ssl.SSLException) "电脑证书校验失败，请在电脑上更新地址并重新生成二维码。"
                            else "配对失败，请检查电脑地址、局域网及二维码有效期。"
                        update()
                    }
                }
            } finally { client?.close() }
        }
    }

    fun press(): Boolean {
        if (!active || pairing || holding || computer == null) return false
        if (activity.checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            activity.requestPermissions(arrayOf(Manifest.permission.RECORD_AUDIO), MainActivity.MICROPHONE_PERMISSION)
            return false
        }
        holding = true; qualified = false
        val sequence = ++pressSequence
        val attempt = generation
        pendingEnter?.let { link?.control(JSONObject().put("type", "cancel").put("session_id", it.toString())); pendingEnter = null }
        if (phase == Phase.DRAINING) resumeAfterDrain = true
        main.postDelayed({
            if (!destroyed && active && holding && sequence == pressSequence && attempt == generation) {
                qualified = true; prepare()
            }
        }, 150)
        update(); return true
    }

    private fun prepare() {
        if (!holding || !qualified || !active) return
        if (phase == Phase.DRAINING) { resumeAfterDrain = true; status = "正在结束上一轮，请保持按住。"; update(); return }
        if (phase == Phase.PREPARING) return
        val attempt = generation
        phase = Phase.PREPARING; status = if (connected) "正在准备麦克风。" else "正在准备麦克风和电脑连接。"; update()
        io.execute {
            try {
                val mic = Microphone.prepare(activity)
                main.post {
                    if (destroyed || attempt != generation || !active || !holding) { mic.cancel(); return@post }
                    microphone = mic
                    if (connected && link?.sampleRate == mic.sampleRate) { begin(); return@post }
                    link?.close(); connected = false
                    val saved = computer ?: run { stop("请重新扫码配对电脑。"); return@post }
                    try {
                        val next = SocketLink(saved, mic.sampleRate,
                            onReady = { source, enter -> ui {
                                if (attempt == generation && link === source && holding && active) { connected = true; enterSupported = enter; begin() }
                            } },
                            onMessage = { source, event -> ui { if (link === source) receive(event) } },
                            onFailure = { source, reason -> ui { if (link === source) stop(reason) } })
                        link = next; next.connect()
                    } catch (_: Exception) { stop("电脑证书或连接信息失效，请重新扫码配对。") }
                }
            } catch (_: Exception) { ui { if (attempt == generation) stop("麦克风启动失败，请检查录音权限或音频设备。") } }
        }
    }

    private fun begin() {
        val mic = microphone ?: return
        val current = link ?: return
        if (!active || !holding || !qualified || !connected || session != null || phase == Phase.DRAINING) return
        val id = UUID.randomUUID(); val attempt = generation
        session = id; pendingEnter = null; phase = Phase.PRIMING; status = "准备中，请开始说话。"
        if (!current.control(JSONObject().put("type", "start").put("session_id", id.toString()))) { stop("电脑连接已断开。"); return }
        mic.start(id, onFrame = { current.audio(it) }, onLevel = { value -> ui {
            if (generation == attempt && microphone === mic && session == id && holding) { level = value; update() }
        } }, onFinished = { last, mode -> ui {
            if (generation == attempt && microphone === mic && session == id && link === current && phase == Phase.DRAINING) {
                microphone = null; level = 0.0
                val message = JSONObject().put("session_id", id.toString())
                if (mode == Microphone.End.CANCEL) message.put("type", "cancel")
                else {
                    message.put("type", "end").put("last_seq", last ?: JSONObject.NULL)
                    if (enterSupported) message.put("action", if (finishOnly) "finish" else "enter")
                }
                current.control(message); update()
            }
        } }, onFailure = { reason -> ui { if (generation == attempt && microphone === mic) stop(reason) } })
        update()
    }

    private var finishOnly = true
    fun release(onlyFinish: Boolean) {
        val wasQualified = qualified
        holding = false; qualified = false; pressSequence++
        if (phase == Phase.DRAINING) { update(); return }
        val id = session
        if (id == null) {
            if (phase == Phase.PREPARING) stop("连接准备已取消，请重新按住。")
            else if (!wasQualified) { status = "按住说话即可开始。"; update() }
            return
        }
        val cancel = phase == Phase.PRIMING
        if (!cancel && phase != Phase.STREAMING) { stop("本次输入已取消，请重新按住。"); return }
        finishOnly = onlyFinish || !enterSupported
        phase = Phase.DRAINING; status = if (cancel) "正在取消本次短按。" else "正在完成输入。"
        microphone?.finish(cancel)
        val attempt = generation
        main.postDelayed({ if (!destroyed && generation == attempt && session == id) stop("结束确认超时，本次输入已停止。") }, 1500)
        update()
    }

    private fun receive(event: JSONObject) {
        val id = runCatching { UUID.fromString(event.optString("session_id")) }.getOrNull() ?: return
        when (event.optString("type")) {
            "ready" -> if (session == id && phase == Phase.PRIMING) { phase = Phase.STREAMING; status = "正在向电脑输入声音。"; update() }
            "ended" -> if (session == id) {
                microphone?.cancel(); microphone = null; level = 0.0; session = null; phase = Phase.IDLE
                val complete = event.optBoolean("complete")
                pendingEnter = if (complete && event.optBoolean("enter_pending")) id else null
                if (resumeAfterDrain) { pendingEnter?.let { link?.control(JSONObject().put("type", "cancel").put("session_id", it.toString())); pendingEnter = null } }
                status = if (!complete) "本次输入已取消或不完整，请重新按住。"
                    else if (pendingEnter != null) "语音已结束，等待电脑发送回车。" else "本次语音已结束，请在电脑确认输入结果。"
                val resume = resumeAfterDrain; resumeAfterDrain = false
                if (holding && resume) { if (qualified) prepare() }
                else if (holding) { holding = false; qualified = false; pressSequence++ }
                update()
            }
            "action_result" -> if (pendingEnter == id && session == null) {
                pendingEnter = null
                status = when (event.optString("status")) { "executed" -> "电脑已发送回车。"; "skipped" -> "已跳过回车，请在电脑确认输入结果。"; else -> "回车未能完成，请在电脑确认。" }
                update()
            }
        }
    }

    fun stop(reason: String) {
        generation++; pressSequence++; holding = false; qualified = false; resumeAfterDrain = false
        session?.let { link?.control(JSONObject().put("type", "cancel").put("session_id", it.toString())) }
        pendingEnter?.let { link?.control(JSONObject().put("type", "cancel").put("session_id", it.toString())) }
        microphone?.cancel(); microphone = null; level = 0.0
        link?.close(); link = null; connected = false; enterSupported = false; session = null; pendingEnter = null
        pairingClient?.close(); pairingClient = null; pairing = false
        phase = Phase.IDLE; status = reason; update()
    }
    fun destroy() { stop("输入已结束。"); destroyed = true; main.removeCallbacksAndMessages(null); io.shutdownNow() }
}
