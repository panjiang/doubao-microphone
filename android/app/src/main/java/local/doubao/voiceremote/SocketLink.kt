package local.doubao.voiceremote

import android.os.SystemClock
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString.Companion.toByteString
import org.json.JSONObject
import java.util.ArrayDeque
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit

/** One ordered socket. Both queued bytes and the age of the oldest audio are bounded. */
class SocketLink(val computer: Computer, val sampleRate: Int,
                 private val onReady: (SocketLink, Boolean) -> Unit,
                 private val onMessage: (SocketLink, JSONObject) -> Unit,
                 private val onFailure: (SocketLink, String) -> Unit) {
    private val client = computer.client()
    private val clock = Executors.newSingleThreadScheduledExecutor()
    private var socket: WebSocket? = null
    private var closed = false
    private var ready = false
    private var received = SystemClock.elapsedRealtime()
    private var opened = received
    private data class Pending(val end: Long, val bytes: Long, val time: Long, val audio: Boolean)
    private val pending = ArrayDeque<Pending>()
    private var total = 0L

    @Synchronized fun connect() {
        if (closed) return
        opened = SystemClock.elapsedRealtime(); received = opened
        socket = client.newWebSocket(Request.Builder().url("wss://${computer.host}:${computer.port}/native/ws").build(), object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                synchronized(this@SocketLink) {
                    if (closed) { webSocket.cancel(); return }
                    socket = webSocket
                    control(JSONObject().put("type", "hello").put("version", 1).put("token", computer.token)
                        .put("sample_rate", sampleRate).put("channels", 1))
                }
            }
            override fun onMessage(webSocket: WebSocket, text: String) {
                synchronized(this@SocketLink) {
                    if (closed) return
                    try {
                        require(text.toByteArray(Charsets.UTF_8).size <= 65536)
                        val value = JSONObject(text)
                        received = SystemClock.elapsedRealtime()
                        when (value.getString("type")) {
                            "error" -> { fail(value.optString("message", "电脑拒绝本次输入。")); return }
                            "hello" -> {
                                require(!ready && value.getInt("version") == 1)
                                ready = true
                                val capabilities = value.optJSONArray("capabilities")
                                val enter = capabilities != null && (0 until capabilities.length()).any { capabilities.optString(it) == "release_enter" }
                                onReady(this@SocketLink, enter)
                            }
                        }
                        onMessage(this@SocketLink, value)
                    } catch (_: Exception) { fail("电脑响应无法识别，请更新程序或重新配对。") }
                }
            }
            override fun onMessage(webSocket: WebSocket, bytes: okio.ByteString) { fail("电脑响应格式不支持。") }
            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) { fail("电脑连接已关闭，请重新按住。") }
            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) { fail("连接失败，请检查电脑程序、局域网及证书，或重新扫码。") }
        })
        clock.scheduleWithFixedDelay({ tick() }, 250, 250, TimeUnit.MILLISECONDS)
    }

    @Synchronized private fun checkQueue(audioBytes: Int = 0): Boolean {
        val ws = socket ?: return false
        val now = SystemClock.elapsedRealtime()
        val drained = total - ws.queueSize()
        while (pending.isNotEmpty() && pending.first.end <= drained) pending.removeFirst()
        val audio = pending.filter { it.audio }
        val queuedAudio = audio.sumOf { (it.end - maxOf(drained, it.end - it.bytes)).coerceAtLeast(0) }
        if (pending.size >= 64 || queuedAudio + audioBytes > (sampleRate * 2L / 5) + 240 || audio.any { now - it.time > 200 }) {
            fail("发送积压超过 200ms，本次输入已停止。"); return false
        }
        return true
    }

    @Synchronized fun control(value: JSONObject): Boolean {
        if (closed || !checkQueue()) return false
        val text = value.toString()
        val size = text.toByteArray(Charsets.UTF_8).size
        if (socket?.send(text) != true) { fail("电脑连接发送失败。"); return false }
        total += size
        pending.add(Pending(total, size.toLong(), SystemClock.elapsedRealtime(), false))
        return true
    }

    @Synchronized fun audio(bytes: ByteArray): Boolean {
        if (closed || !ready || !checkQueue(bytes.size)) return false
        if (socket?.send(bytes.toByteString()) != true) { fail("音频发送失败，请重新按住。"); return false }
        total += bytes.size
        pending.add(Pending(total, bytes.size.toLong(), SystemClock.elapsedRealtime(), true))
        return true
    }

    @Synchronized private fun tick() {
        if (closed) return
        val now = SystemClock.elapsedRealtime()
        if (!ready && now - opened > 5000) { fail("连接电脑超时，请检查电脑程序和局域网。"); return }
        if (ready && now - received >= 1000) { fail("电脑响应超时，本次输入已停止，请重新按住。"); return }
        if (!checkQueue()) return
        if (ready) control(JSONObject().put("type", "heartbeat"))
    }

    @Synchronized private fun fail(message: String) {
        if (closed) return
        close(); onFailure(this, message)
    }

    @Synchronized fun close() {
        if (closed) return
        closed = true; clock.shutdownNow(); pending.clear()
        socket?.cancel(); socket = null
        client.dispatcher.cancelAll(); client.connectionPool.evictAll(); client.dispatcher.executorService.shutdown()
    }
}
