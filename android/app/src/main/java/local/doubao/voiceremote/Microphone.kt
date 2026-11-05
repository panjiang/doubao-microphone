package local.doubao.voiceremote

import android.annotation.SuppressLint
import android.content.Context
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioRecord
import android.media.AudioRecordingConfiguration
import android.media.AudioRouting
import android.media.MediaRecorder
import android.os.Handler
import android.os.Looper
import android.os.Process
import android.os.SystemClock
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.UUID
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference
import kotlin.math.sqrt

class Microphone private constructor(private val record: AudioRecord) {
    enum class End { FINISH, CANCEL, ABORT }
    val sampleRate = record.sampleRate
    private val ending = AtomicReference<End?>(null)
    private val started = AtomicBoolean(false)
    private val released = AtomicBoolean(false)
    private val main = Handler(Looper.getMainLooper())
    @Volatile private var routed: Int? = null

    fun start(id: UUID, onFrame: (ByteArray) -> Boolean, onLevel: (Double) -> Unit,
              onFinished: (Long?, End) -> Unit, onFailure: (String) -> Unit) {
        check(started.compareAndSet(false, true))
        Thread({
            runCatching { Process.setThreadPriority(Process.THREAD_PRIORITY_AUDIO) }
            var last: Long? = null
            val failure = AtomicReference<String?>(null)
            val route = AudioRouting.OnRoutingChangedListener {
                if (released.get() || ending.get() != null) return@OnRoutingChangedListener
                val next = runCatching { record.routedDevice?.id }.getOrNull()
                if (routed != null && next != routed && ending.get() == null) {
                    failure.set("音频设备已变化，请重新按住。"); cancel()
                } else routed = next
            }
            val state = object : AudioManager.AudioRecordingCallback() {
                override fun onRecordingConfigChanged(configs: MutableList<AudioRecordingConfiguration>) {
                    if (released.get() || ending.get() != null) return
                    if (runCatching { record.activeRecordingConfiguration?.isClientSilenced }.getOrNull() == true) {
                        onFailure("麦克风被系统暂停，请检查来电或麦克风隐私开关。"); cancel()
                    }
                }
            }
            try {
                if (ending.get() == End.ABORT) return@Thread
                record.addOnRoutingChangedListener(route, main)
                record.registerAudioRecordingCallback({ command -> main.post(command) }, state)
                record.startRecording()
                check(record.recordingState == AudioRecord.RECORDSTATE_RECORDING)
                routed = record.routedDevice?.id
                val frame = ShortArray(sampleRate / 50)
                var sequence = 0L
                var lastMeter = 0L
                var tailFrames = 0
                while (true) {
                    val mode = ending.get()
                    if (mode == End.ABORT || mode == End.CANCEL) break
                    val before = SystemClock.elapsedRealtime()
                    val n = record.read(frame, 0, frame.size, if (mode == End.FINISH) AudioRecord.READ_NON_BLOCKING else AudioRecord.READ_BLOCKING)
                    if (ending.get() == End.ABORT || ending.get() == End.CANCEL) break
                    if (n < 0) error("手机采音中断，请重新按住。")
                    if (n == 0) {
                        if (mode == End.FINISH) break
                        Thread.sleep(5); continue
                    }
                    require(SystemClock.elapsedRealtime() - before <= 200) { "采音处理延迟超过 200ms，请重新按住。" }
                    if (mode == End.FINISH && ++tailFrames > 10) error("音频尾部积压，本次输入已取消。")
                    val packet = ByteBuffer.allocate(24 + n * 2)
                    packet.put(byteArrayOf(80, 80, 84, 49)).putLong(id.mostSignificantBits).putLong(id.leastSignificantBits)
                    packet.order(ByteOrder.LITTLE_ENDIAN).putInt(sequence.toInt())
                    for (i in 0 until n) packet.putShort(frame[i])
                    check(onFrame(packet.array())) { "音频发送已停止。" }
                    last = sequence++
                    val now = SystemClock.elapsedRealtime()
                    if (now - lastMeter >= 80) {
                        var energy = 0.0
                        for (i in 0 until n) { val sample = frame[i] / 32768.0; energy += sample * sample }
                        onLevel((sqrt(energy / n) * 5).coerceIn(0.0, 1.0)); lastMeter = now
                    }
                }
            } catch (e: Exception) {
                if (ending.get() != End.ABORT && ending.get() != End.CANCEL) failure.set(e.message ?: "手机采音失败，请重新按住。")
            } finally {
                runCatching { record.removeOnRoutingChangedListener(route) }
                runCatching { record.unregisterAudioRecordingCallback(state) }
                runCatching { record.stop() }; release()
                if (failure.get() != null) onFailure(failure.get()!!)
                else if (ending.get() == End.FINISH || ending.get() == End.CANCEL) onFinished(last, ending.get()!!)
            }
        }, "doubao-microphone").start()
    }

    fun finish(cancel: Boolean = false) { ending.compareAndSet(null, if (cancel) End.CANCEL else End.FINISH) }
    fun cancel() {
        ending.set(End.ABORT)
        if (started.get()) runCatching { record.stop() } else release()
    }
    private fun release() { if (released.compareAndSet(false, true)) record.release() }

    companion object {
        @SuppressLint("MissingPermission")
        fun prepare(context: Context): Microphone {
            val manager = context.getSystemService(AudioManager::class.java)
            val source = if (manager.getProperty(AudioManager.PROPERTY_SUPPORT_AUDIO_SOURCE_UNPROCESSED) == "true")
                MediaRecorder.AudioSource.UNPROCESSED else MediaRecorder.AudioSource.VOICE_RECOGNITION
            for (rate in listOf(48000, 44100, 24000)) {
                val min = AudioRecord.getMinBufferSize(rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
                if (min <= 0) continue
                var record: AudioRecord? = null
                try {
                    record = AudioRecord.Builder().setAudioSource(source).setAudioFormat(AudioFormat.Builder()
                        .setSampleRate(rate).setEncoding(AudioFormat.ENCODING_PCM_16BIT).setChannelMask(AudioFormat.CHANNEL_IN_MONO).build())
                        .setBufferSizeInBytes(maxOf(min * 2, rate * 2 / 5)).build()
                    if (record.state == AudioRecord.STATE_INITIALIZED && record.sampleRate in 8000..96000 && record.sampleRate % 50 == 0) return Microphone(record)
                } catch (_: Exception) { }
                record?.release()
            }
            error("无法初始化麦克风，请检查录音权限和音频设备。")
        }
    }
}
