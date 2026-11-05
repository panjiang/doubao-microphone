package local.doubao.voiceremote

import android.Manifest
import android.app.Activity
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.graphics.Typeface
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.view.WindowManager
import android.widget.Button
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView

class MainActivity : Activity() {
    private lateinit var controller: RemoteController
    private lateinit var surface: HoldSurface
    private lateinit var badge: TextView
    private lateinit var scan: Button
    private var pendingQr: String? = null
    private fun dp(value: Int) = (value * resources.displayMetrics.density).toInt()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val root = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setBackgroundColor(Color.rgb(242, 245, 250)) }
        root.setOnApplyWindowInsetsListener { view, insets ->
            view.setPadding(insets.systemWindowInsetLeft, insets.systemWindowInsetTop, insets.systemWindowInsetRight, insets.systemWindowInsetBottom)
            insets.consumeSystemWindowInsets()
        }
        val header = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL; setPadding(dp(16), dp(8), dp(16), dp(8)) }
        val title = TextView(this).apply { text = "豆包遥控"; textSize = 16f; gravity = Gravity.CENTER_VERTICAL; setTextColor(Color.rgb(28, 35, 48)); setTypeface(typeface, Typeface.BOLD) }
        header.addView(title, LinearLayout.LayoutParams(0, dp(40), 1f).apply { gravity = Gravity.CENTER_VERTICAL })
        scan = Button(this).apply { text = "扫码"; textSize = 13f; setOnClickListener { openScanner() }; contentDescription = "扫码连接电脑" }
        header.addView(scan, LinearLayout.LayoutParams(dp(76), dp(44)))
        badge = TextView(this).apply { textSize = 12f; setPadding(dp(10), dp(6), dp(10), dp(6)); setTextColor(Color.rgb(67, 80, 101)) }
        header.addView(badge)
        root.addView(header)
        root.addView(View(this).apply { setBackgroundColor(Color.rgb(218, 223, 233)) }, LinearLayout.LayoutParams(-1, dp(1)))
        surface = HoldSurface(this)
        root.addView(surface, LinearLayout.LayoutParams(-1, 0, 1f))
        setContentView(root)
        controller = RemoteController(this) { render() }
        surface.onDown = { controller.press() }
        surface.onUp = { controller.release(it) }
        surface.onCancel = { controller.stop("触摸已取消，本次输入已停止。") }
        surface.onScan = { openScanner() }
        render()
    }
    private fun render() {
        badge.text = if (controller.computer == null) "未配对" else "已配对"
        scan.isEnabled = !controller.pairing && !controller.holding
        surface.update(controller.computer?.name ?: "连接你的电脑", controller.status, controller.computer != null,
            controller.pairing, controller.holding, controller.phase, controller.level)
    }
    private fun openScanner() {
        controller.stop("请扫描电脑配置页的配对二维码。")
        if (checkSelfPermission(Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED) {
            startActivityForResult(Intent(this, ScannerActivity::class.java), SCAN_RESULT)
        } else requestPermissions(arrayOf(Manifest.permission.CAMERA), CAMERA_PERMISSION)
    }
    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<out String>, grantResults: IntArray) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        val granted = grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED
        if (requestCode == MICROPHONE_PERMISSION) controller.permissionResult(granted)
        if (requestCode == CAMERA_PERMISSION) {
            // The scanner also offers manual paste if the camera permission is denied.
            startActivityForResult(Intent(this, ScannerActivity::class.java), SCAN_RESULT)
        }
    }
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == SCAN_RESULT && resultCode == RESULT_OK) pendingQr = data?.getStringExtra("qr")
    }
    override fun onResume() {
        super.onResume(); window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        if (::controller.isInitialized) {
            controller.foreground(true)
            pendingQr?.let { pendingQr = null; controller.pair(it) }
        }
    }
    override fun onPause() {
        if (::controller.isInitialized) controller.foreground(false)
        window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        super.onPause()
    }
    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (!hasFocus && ::controller.isInitialized && controller.holding) controller.stop("输入被系统界面打断，请重新按住。")
    }
    override fun onDestroy() { if (::controller.isInitialized) controller.destroy(); super.onDestroy() }
    companion object { const val MICROPHONE_PERMISSION = 20; private const val CAMERA_PERMISSION = 21; private const val SCAN_RESULT = 22 }
}
