package local.doubao.voiceremote

import android.Manifest
import android.annotation.SuppressLint
import android.app.Activity
import android.app.AlertDialog
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.graphics.Matrix
import android.graphics.SurfaceTexture
import android.hardware.camera2.CameraCaptureSession
import android.hardware.camera2.CameraCharacteristics
import android.hardware.camera2.CameraDevice
import android.hardware.camera2.CameraManager
import android.hardware.camera2.CaptureRequest
import android.media.ImageReader
import android.os.Bundle
import android.os.Handler
import android.os.HandlerThread
import android.os.SystemClock
import android.util.Size
import android.view.Gravity
import android.view.Surface
import android.view.TextureView
import android.view.WindowManager
import android.widget.Button
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.TextView
import com.google.zxing.BarcodeFormat
import com.google.zxing.BinaryBitmap
import com.google.zxing.DecodeHintType
import com.google.zxing.MultiFormatReader
import com.google.zxing.PlanarYUVLuminanceSource
import com.google.zxing.common.HybridBinarizer

class ScannerActivity : Activity(), TextureView.SurfaceTextureListener {
    private lateinit var preview: TextureView
    private lateinit var hint: TextView
    private var thread: HandlerThread? = null
    private var handler: Handler? = null
    private var camera: CameraDevice? = null
    private var session: CameraCaptureSession? = null
    private var reader: ImageReader? = null
    private var surface: Surface? = null
    @Volatile private var active = false
    @Volatile private var finished = false
    private var opening = false
    private var lastDecode = 0L

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val root = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setBackgroundColor(Color.rgb(242, 245, 250)) }
        root.setOnApplyWindowInsetsListener { view, insets ->
            view.setPadding(insets.systemWindowInsetLeft, insets.systemWindowInsetTop, insets.systemWindowInsetRight, insets.systemWindowInsetBottom)
            insets.consumeSystemWindowInsets()
        }
        hint = TextView(this).apply { text = "对准电脑配置页的配对二维码"; textSize = 16f; gravity = Gravity.CENTER; setPadding(16, 20, 16, 20) }
        root.addView(hint)
        preview = TextureView(this).apply { surfaceTextureListener = this@ScannerActivity }
        root.addView(preview, LinearLayout.LayoutParams(-1, 0, 1f))
        root.addView(Button(this).apply { text = "粘贴配对链接"; setOnClickListener { paste() } })
        root.addView(Button(this).apply { text = "取消"; setOnClickListener { finish() } })
        setContentView(root)
    }
    private fun paste() {
        val input = EditText(this).apply { hint = "粘贴电脑配置页二维码中的完整链接"; setSingleLine(false); maxLines = 5 }
        AlertDialog.Builder(this).setTitle("配对链接").setView(input).setNegativeButton("取消", null)
            .setPositiveButton("连接") { _, _ -> accept(input.text.toString().trim()) }.show()
    }
    private fun accept(value: String) {
        if (finished) return
        try { PairPayload.parse(value) } catch (_: Exception) { hint.text = "不是有效配对二维码，请重新生成后扫码。"; return }
        finished = true; active = false
        setResult(RESULT_OK, Intent().putExtra("qr", value)); finish()
    }
    override fun onResume() {
        super.onResume(); active = true
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        thread = HandlerThread("doubao-qr-camera").apply { start() }; handler = Handler(thread!!.looper)
        if (preview.isAvailable) openCamera()
    }
    override fun onPause() {
        active = false; opening = false
        session?.close(); session = null; camera?.close(); camera = null
        reader?.close(); reader = null; surface?.release(); surface = null
        thread?.quitSafely(); thread = null; handler = null
        window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        super.onPause()
    }
    @SuppressLint("MissingPermission")
    private fun openCamera() {
        if (!active || finished || opening || camera != null || !preview.isAvailable) return
        if (checkSelfPermission(Manifest.permission.CAMERA) != PackageManager.PERMISSION_GRANTED) {
            hint.text = "没有相机权限，可在下方粘贴配对链接。"; return
        }
        val background = handler ?: return
        opening = true
        try {
            val manager = getSystemService(Context.CAMERA_SERVICE) as CameraManager
            val id = manager.cameraIdList.firstOrNull { manager.getCameraCharacteristics(it).get(CameraCharacteristics.LENS_FACING) == CameraCharacteristics.LENS_FACING_BACK }
                ?: manager.cameraIdList.firstOrNull() ?: error("没有可用相机。")
            val characteristics = manager.getCameraCharacteristics(id)
            val sizes = characteristics.get(CameraCharacteristics.SCALER_STREAM_CONFIGURATION_MAP)!!
                .getOutputSizes(android.graphics.ImageFormat.YUV_420_888)
            val size = sizes.filter { it.width <= 1280 && it.height <= 960 }.maxByOrNull { it.width * it.height }
                ?: sizes.minBy { it.width * it.height }
            preview.surfaceTexture!!.setDefaultBufferSize(size.width, size.height)
            surface = Surface(preview.surfaceTexture)
            val decoder = MultiFormatReader().apply { setHints(mapOf(DecodeHintType.POSSIBLE_FORMATS to listOf(BarcodeFormat.QR_CODE), DecodeHintType.TRY_HARDER to true)) }
            reader = ImageReader.newInstance(size.width, size.height, android.graphics.ImageFormat.YUV_420_888, 2).apply {
                setOnImageAvailableListener({ source ->
                    val image = runCatching { source.acquireLatestImage() }.getOrNull() ?: return@setOnImageAvailableListener
                    try {
                        val now = SystemClock.elapsedRealtime()
                        if (!active || finished || handler !== background || now - lastDecode < 200) return@setOnImageAvailableListener
                        lastDecode = now
                        val plane = image.planes[0]; val bytes = ByteArray(image.width * image.height)
                        val buffer = plane.buffer
                        val origin = buffer.position()
                        for (y in 0 until image.height) for (x in 0 until image.width) {
                            bytes[y * image.width + x] = buffer.get(origin + y * plane.rowStride + x * plane.pixelStride)
                        }
                        val luminance = PlanarYUVLuminanceSource(bytes, image.width, image.height, 0, 0, image.width, image.height, false)
                        val result = decoder.decodeWithState(BinaryBitmap(HybridBinarizer(luminance)))
                        runOnUiThread { if (active && !finished) accept(result.text) }
                    } catch (_: Exception) { } finally { decoder.reset(); image.close() }
                }, background)
            }
            val output = reader!!; val viewSurface = surface!!
            val sensor = characteristics.get(CameraCharacteristics.SENSOR_ORIENTATION) ?: 90
            val matrix = Matrix().apply {
                val rotated = sensor == 90 || sensor == 270
                val natural = if (rotated) size.height.toFloat() / size.width else size.width.toFloat() / size.height
                val actual = preview.width.toFloat() / preview.height.coerceAtLeast(1)
                if (actual > natural) setScale(1f, actual / natural, preview.width / 2f, preview.height / 2f)
                else setScale(natural / actual, 1f, preview.width / 2f, preview.height / 2f)
            }
            preview.setTransform(matrix)
            manager.openCamera(id, object : CameraDevice.StateCallback() {
                override fun onOpened(device: CameraDevice) {
                    if (!active || finished || handler !== background) { device.close(); return }
                    camera = device; opening = false
                    try {
                        device.createCaptureSession(listOf(viewSurface, output.surface), object : CameraCaptureSession.StateCallback() {
                            override fun onConfigured(value: CameraCaptureSession) {
                                if (!active || camera !== device || handler !== background) { value.close(); return }
                                session = value
                                val request = device.createCaptureRequest(CameraDevice.TEMPLATE_PREVIEW).apply {
                                    addTarget(viewSurface); addTarget(output.surface)
                                    set(CaptureRequest.CONTROL_AF_MODE, CaptureRequest.CONTROL_AF_MODE_CONTINUOUS_PICTURE)
                                }
                                runCatching { value.setRepeatingRequest(request.build(), null, background) }
                                    .onFailure { showFailure() }
                            }
                            override fun onConfigureFailed(value: CameraCaptureSession) { showFailure() }
                        }, Handler(mainLooper))
                    } catch (_: Exception) { showFailure() }
                }
                override fun onDisconnected(device: CameraDevice) { device.close(); showFailure() }
                override fun onError(device: CameraDevice, error: Int) { device.close(); showFailure() }
            }, Handler(mainLooper))
        } catch (_: Exception) { opening = false; showFailure() }
    }
    private fun showFailure() { runOnUiThread { if (active && !finished) hint.text = "无法打开相机，可在下方粘贴配对链接。" } }
    override fun onSurfaceTextureAvailable(texture: SurfaceTexture, width: Int, height: Int) { openCamera() }
    override fun onSurfaceTextureSizeChanged(texture: SurfaceTexture, width: Int, height: Int) { }
    override fun onSurfaceTextureDestroyed(texture: SurfaceTexture): Boolean = true
    override fun onSurfaceTextureUpdated(texture: SurfaceTexture) { }
}
