package local.doubao.voiceremote

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.AtomicFile
import android.util.Base64
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import org.json.JSONObject
import java.io.ByteArrayInputStream
import java.io.File
import java.net.URI
import java.net.URLDecoder
import java.security.KeyStore
import java.security.cert.CertificateFactory
import java.security.cert.X509Certificate
import java.util.UUID
import java.util.concurrent.TimeUnit
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import javax.net.ssl.SSLContext
import javax.net.ssl.TrustManagerFactory
import javax.net.ssl.X509TrustManager

data class Computer(val host: String, val port: Int, val ca: ByteArray, val token: String = "", val name: String = "我的电脑") {
    fun certificate(): X509Certificate = CertificateFactory.getInstance("X.509")
        .generateCertificate(ByteArrayInputStream(ca)) as X509Certificate

    fun validate() {
        val parts = host.split('.')
        require(parts.size == 4 && parts.all { it.matches(Regex("0|[1-9][0-9]{0,2}")) && it.toInt() <= 255 }
            && !host.startsWith("127.") && host != "0.0.0.0" && host != "255.255.255.255"
            && parts.first().toInt() < 224 && port in 1024..65535 && ca.size in 1..8192) { "电脑地址或证书无效，请重新扫码。" }
        val cert = certificate()
        cert.checkValidity()
        require(cert.basicConstraints >= 0) { "二维码没有有效的电脑 CA 证书。" }
    }

    fun url(path: String) = "https://$host:$port$path"
    fun json() = JSONObject().put("host", host).put("port", port).put("ca_der", Base64.encodeToString(ca, Base64.NO_WRAP))
        .put("token", token).put("name", name)

    fun client(): OkHttpClient {
        validate()
        val store = KeyStore.getInstance(KeyStore.getDefaultType()).apply { load(null); setCertificateEntry("computer", certificate()) }
        val trust = TrustManagerFactory.getInstance(TrustManagerFactory.getDefaultAlgorithm()).apply { init(store) }
            .trustManagers.filterIsInstance<X509TrustManager>().single()
        val ssl = SSLContext.getInstance("TLS").apply { init(null, arrayOf(trust), null) }
        return OkHttpClient.Builder().sslSocketFactory(ssl.socketFactory, trust)
            // OkHttp's default hostname verifier still checks the IP SAN. No system trust changes.
            .followRedirects(false).followSslRedirects(false).retryOnConnectionFailure(false)
            .connectTimeout(5, TimeUnit.SECONDS).readTimeout(8, TimeUnit.SECONDS).callTimeout(8, TimeUnit.SECONDS).build()
    }

    companion object {
        fun fromJson(value: JSONObject) = Computer(value.getString("host"), value.getInt("port"),
            Base64.decode(value.getString("ca_der"), Base64.DEFAULT), value.optString("token"), value.optString("name", "我的电脑")).also { it.validate() }
    }
}

data class PairPayload(val computer: Computer, val ticket: String) {
    companion object {
        private fun fields(text: String): Map<String, String> {
            val fields = text.split('&').map {
                val pair = it.split('=', limit = 2)
                require(pair.size == 2) { "配对二维码参数无效。" }
                URLDecoder.decode(pair[0], "UTF-8") to URLDecoder.decode(pair[1], "UTF-8")
            }
            require(fields.map { it.first }.toSet().size == fields.size) { "二维码参数重复，请重新生成。" }
            return fields.toMap()
        }

        fun parse(text: String): PairPayload {
            require(text.toByteArray(Charsets.UTF_8).size <= 16384) { "二维码过大或无效。" }
            val result = if (text.startsWith("{")) {
                val j = JSONObject(text)
                require(j.getString("type") == "doubao_ptt_pair" && j.getInt("version") == 1) { "二维码类型不支持。" }
                PairPayload(Computer.fromJson(j), j.getString("ticket"))
            } else {
                val u = URI(text)
                require(u.scheme == "http" && u.rawUserInfo == null && u.path == "/setup" && u.port in 1024..65535) { "请扫描电脑配置页生成的二维码。" }
                val query = fields(u.rawQuery ?: "")
                val fragment = fields(u.rawFragment ?: "")
                require(query.keys == setOf("token") && fragment.keys == setOf("v", "port", "ca") && fragment["v"] == "1") { "配对二维码版本或参数无效。" }
                val ca = fragment.getValue("ca")
                require(ca.matches(Regex("[A-Za-z0-9_-]+")) && ca.length % 4 != 1) { "二维码证书格式无效。" }
                val computer = Computer(u.host ?: "", fragment.getValue("port").toInt(), Base64.decode(ca, Base64.URL_SAFE or Base64.NO_PADDING or Base64.NO_WRAP))
                computer.validate()
                PairPayload(computer, query.getValue("token"))
            }
            require(result.ticket.length in 32..256 && result.ticket.all { it.code in 33..126 }) { "配对票据无效，请重新生成二维码。" }
            return result
        }
    }
}

class PairStore(context: Context) {
    private val file = AtomicFile(File(context.noBackupFilesDir, "computer-v1.bin"))
    private fun key(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        val alias = "doubao-voice-remote-computer-v1"
        (store.getKey(alias, null) as? SecretKey)?.let { return it }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
            init(KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256).build())
        }.generateKey()
    }

    fun load(): Computer? {
        if (!file.baseFile.exists()) return null
        val bytes = file.readFully()
        require(bytes.size in 30..32768 && bytes[0].toInt() == 1) { "配对记录无法读取，请重新扫码。" }
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, bytes.copyOfRange(1, 13))) }
        return Computer.fromJson(JSONObject(String(cipher.doFinal(bytes.copyOfRange(13, bytes.size)), Charsets.UTF_8))).also {
            require(it.token.length in 32..512) { "配对凭据无效，请重新扫码。" }
        }
    }

    fun save(computer: Computer) {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        val bytes = byteArrayOf(1) + cipher.iv + cipher.doFinal(computer.json().toString().toByteArray(Charsets.UTF_8))
        val output = file.startWrite()
        try { output.write(bytes); file.finishWrite(output) } catch (e: Exception) { file.failWrite(output); throw e }
    }
}

class PairingClient(private val computer: Computer) {
    private val client = computer.client()
    @Volatile private var closed = false
    fun close() { closed = true; client.dispatcher.cancelAll(); client.connectionPool.evictAll(); client.dispatcher.executorService.shutdown() }
    private fun json(path: String, body: JSONObject? = null): JSONObject {
        check(!closed) { "配对已取消。" }
        val request = Request.Builder().url(computer.url(path))
        if (body != null) request.post(body.toString().toRequestBody("application/json".toMediaType()))
        client.newCall(request.build()).execute().use { response ->
            val source = response.body?.source() ?: error("电脑未返回数据。")
            require(!source.request(16385)) { "电脑响应过大。" }
            val value = JSONObject(source.readUtf8())
            require(response.isSuccessful) { value.optString("error", "电脑拒绝配对，请重新生成二维码。") }
            return value
        }
    }
    fun pair(ticket: String): Computer {
        val info = json("/info")
        val pending = json("/native/pair", JSONObject().put("ticket", ticket).put("label", "Android App"))
        val id = pending.getString("id").also { UUID.fromString(it) }
        val poll = pending.getString("poll")
        require(poll.length in 16..512) { "配对响应无效。" }
        val deadline = android.os.SystemClock.elapsedRealtime() + 300000
        while (!closed && android.os.SystemClock.elapsedRealtime() < deadline) {
            val result = json("/native/pair-result/$id?poll=${java.net.URLEncoder.encode(poll, "UTF-8")}")
            val token = result.optString("token")
            if (token.isNotEmpty()) {
                require(token.length in 32..512) { "设备凭据无效。" }
                return computer.copy(token = token, name = info.optString("computer_name", "我的电脑"))
            }
            Thread.sleep(750)
        }
        error("配对超时，请重新生成二维码。")
    }
}
