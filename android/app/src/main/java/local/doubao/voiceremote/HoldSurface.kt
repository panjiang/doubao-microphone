package local.doubao.voiceremote

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.os.SystemClock
import android.text.Layout
import android.text.StaticLayout
import android.text.TextPaint
import android.text.TextUtils
import android.view.MotionEvent
import android.view.View
import android.view.animation.AnimationUtils
import kotlin.math.exp
import kotlin.math.hypot
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sqrt

/** A fixed 60dp hit radius. Decorative wave radii never change the gesture boundary. */
class HoldSurface(context: Context) : View(context) {
    var onDown: (() -> Boolean)? = null
    var onUp: ((Boolean) -> Unit)? = null
    var onCancel: (() -> Unit)? = null
    var onScan: (() -> Unit)? = null
    private val density = resources.displayMetrics.density
    private fun dp(v: Float) = v * density
    private val blue = Color.rgb(35, 75, 198)
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val text = TextPaint(Paint.ANTI_ALIAS_FLAG)
    private val scanRect = RectF()
    private var paired = false
    private var pairing = false
    private var name = "连接你的电脑"
    private var status = "扫描电脑配置页的配对二维码。"
    private var phase = RemoteController.Phase.IDLE
    private var pointer = -1
    private var gesture = false
    private var scanTouch = false
    private var anchorX = 0f
    private var anchorY = 0f
    private var pressTime = 0L
    private var releaseTime = 0L
    private var fadeFrom = 1f
    private var target = 0.0
    private var smoothed = 0.0
    private var lastDraw = 0L
    private var hintFrom = 1f
    private var hintTo = 1f
    private var hintTime = 0L

    init { isClickable = true; isFocusable = true; contentDescription = "顶栏以下按住说话；圆内松手自动按 Enter，圆外松手仅完成输入。" }

    fun update(name: String, status: String, paired: Boolean, pairing: Boolean, holding: Boolean, phase: RemoteController.Phase, level: Double) {
        this.name = name; this.status = status; this.paired = paired; this.pairing = pairing; this.phase = phase
        target = if (level.isFinite()) sqrt(((level - 0.008).coerceAtLeast(0.0) * 4).coerceAtMost(1.0)) else 0.0
        if (gesture && !holding) finish(false)
        invalidate()
    }
    private fun opacity(now: Long): Float = if (gesture) ((now - pressTime) / 300f).coerceIn(0f, 1f)
        else if (releaseTime > 0) fadeFrom * (1f - (now - releaseTime) / 300f).coerceIn(0f, 1f) else 0f
    private fun hintOpacity(now: Long) = hintFrom + (hintTo - hintFrom) * ((now - hintTime) / 300f).coerceIn(0f, 1f)
    private fun hint(show: Boolean) { val now = SystemClock.uptimeMillis(); hintFrom = hintOpacity(now); hintTo = if (show) 1f else 0f; hintTime = now }
    private fun finish(animated: Boolean) {
        val now = SystemClock.uptimeMillis()
        fadeFrom = opacity(now); gesture = false; releaseTime = if (animated) now else 0
        target = 0.0; smoothed = 0.0; hint(true); invalidate()
    }
    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                if (pointer != -1) return true
                pointer = event.getPointerId(0)
                if (!paired) { scanTouch = !pairing && scanRect.contains(event.x, event.y); return true }
                if (pairing) return true
                anchorX = event.x; anchorY = event.y; pressTime = SystemClock.uptimeMillis()
                releaseTime = 0; smoothed = 0.0; target = 0.0; lastDraw = pressTime
                gesture = true; hint(false)
                if (onDown?.invoke() != true) finish(false)
                invalidate(); return true
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP -> {
                if (event.getPointerId(event.actionIndex) != pointer) return true
                pointer = -1
                if (scanTouch) { scanTouch = false; if (scanRect.contains(event.x, event.y)) { performClick(); onScan?.invoke() }; return true }
                if (gesture) {
                    val x = event.getX(event.actionIndex); val y = event.getY(event.actionIndex)
                    val outside = x < 0 || y < 0 || x >= width || y >= height || hypot(x - anchorX, y - anchorY) > dp(60f)
                    finish(true); onUp?.invoke(outside)
                }
                return true
            }
            MotionEvent.ACTION_CANCEL -> {
                pointer = -1; scanTouch = false
                if (gesture) { finish(false); onCancel?.invoke() }
                return true
            }
        }
        return true
    }
    override fun performClick(): Boolean = super.performClick()
    override fun onDetachedFromWindow() {
        if (gesture) onCancel?.invoke()
        gesture = false; releaseTime = 0; pointer = -1
        super.onDetachedFromWindow()
    }
    private fun label(canvas: Canvas, value: String, x: Float, y: Float, size: Float, color: Int, bold: Boolean = false, alpha: Float = 1f) {
        text.textSize = size * resources.displayMetrics.scaledDensity; text.color = color; text.alpha = (alpha * 255).toInt(); text.isFakeBoldText = bold
        canvas.drawText(value, x, y, text)
    }
    private fun mic(canvas: Canvas, x: Float, y: Float, scale: Float, color: Int, alpha: Float) {
        canvas.save(); canvas.translate(x, y); canvas.scale(scale, scale)
        paint.style = Paint.Style.FILL; paint.color = color; paint.alpha = (255 * alpha).toInt()
        canvas.drawRoundRect(-9f, -22f, 9f, 9f, 9f, 9f, paint)
        paint.style = Paint.Style.STROKE; paint.strokeWidth = 3f; paint.strokeCap = Paint.Cap.ROUND
        val path = Path().apply { moveTo(-15f, 3f); lineTo(-15f, 9f); cubicTo(-15f, 28f, 15f, 28f, 15f, 9f); lineTo(15f, 3f); moveTo(0f, 23f); lineTo(0f, 32f); moveTo(-9f, 32f); lineTo(9f, 32f) }
        canvas.drawPath(path, paint); canvas.restore()
    }
    private fun actionIcon(canvas: Canvas, x: Float, y: Float, slash: Boolean) {
        paint.style = Paint.Style.STROKE; paint.strokeWidth = dp(1.5f); paint.color = Color.rgb(119, 130, 147); paint.alpha = 255; paint.strokeCap = Paint.Cap.ROUND
        val path = Path().apply {
            moveTo(x + dp(18f), y - dp(9f)); lineTo(x + dp(18f), y + dp(1f)); quadTo(x + dp(18f), y + dp(8f), x + dp(11f), y + dp(8f))
            lineTo(x, y + dp(8f)); moveTo(x + dp(5f), y + dp(3f)); lineTo(x, y + dp(8f)); lineTo(x + dp(5f), y + dp(13f))
            if (slash) { moveTo(x, y - dp(10f)); lineTo(x + dp(21f), y + dp(14f)) }
        }
        canvas.drawPath(path, paint)
    }
    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        val now = SystemClock.uptimeMillis()
        val voice = phase == RemoteController.Phase.PRIMING || phase == RemoteController.Phase.STREAMING
        val dt = ((now - lastDraw) / 1000.0).coerceIn(0.0, 0.1); lastDraw = now
        val goal = if (voice && gesture) target else 0.0
        smoothed += (goal - smoothed) * (1 - exp(-dt / if (goal > smoothed) 0.06 else 0.22))
        val left = dp(24f)
        text.textSize = 27f * resources.displayMetrics.scaledDensity; text.isFakeBoldText = true
        val shownName = TextUtils.ellipsize(name, text, (width - left * 2).coerceAtLeast(1f), TextUtils.TruncateAt.END).toString()
        label(canvas, shownName, left, dp(48f), 27f, Color.rgb(28, 35, 48), true)
        text.textSize = 14f * resources.displayMetrics.scaledDensity; text.color = Color.rgb(105, 114, 130); text.alpha = 255; text.isFakeBoldText = false
        val layout = StaticLayout.Builder.obtain(status, 0, status.length, text, (width - left * 2).toInt().coerceAtLeast(1)).setAlignment(Layout.Alignment.ALIGN_NORMAL).build()
        canvas.save(); canvas.translate(left, dp(72f)); layout.draw(canvas); canvas.restore()
        if (phase == RemoteController.Phase.STREAMING && gesture) {
            val y = dp(80f) + layout.height
            for ((i, message) in listOf("圆内松手，会自动按 Enter 键。", "圈外松手，不触发任何按键。").withIndex()) {
                actionIcon(canvas, left, y + dp(9f + i * 32f), i == 1)
                label(canvas, message, left + dp(31f), y + dp(16f + i * 32f), 13f, Color.rgb(105, 114, 130))
            }
        }
        val hintAlpha = hintOpacity(now)
        if (paired && hintAlpha > 0) {
            mic(canvas, width / 2f, height * 0.53f - dp(38f), dp(1f), blue, hintAlpha)
            val value = "按住任意位置说话"
            text.textSize = 19f * resources.displayMetrics.scaledDensity
            label(canvas, value, (width - text.measureText(value)) / 2, height * 0.53f + dp(38f), 19f, Color.rgb(105, 114, 130), alpha = hintAlpha)
        }
        if (!paired) {
            scanRect.set(width / 2 - dp(110f), height * 0.53f - dp(24f), width / 2 + dp(110f), height * 0.53f + dp(32f))
            paint.style = Paint.Style.FILL; paint.color = blue; paint.alpha = if (pairing) 110 else 255
            canvas.drawRoundRect(scanRect, dp(14f), dp(14f), paint)
            val value = if (pairing) "正在配对…" else "扫码连接电脑"
            text.textSize = 17f * resources.displayMetrics.scaledDensity
            label(canvas, value, width / 2 - text.measureText(value) / 2, scanRect.centerY() + dp(6f), 17f, Color.WHITE)
        }
        val opacity = opacity(now)
        if (opacity > 0) {
            val reduce = !android.animation.ValueAnimator.areAnimatorsEnabled()
            if (voice && !reduce) {
                val t = (now - pressTime) / 1000f
                paint.style = Paint.Style.STROKE; paint.strokeWidth = dp(4f); paint.color = blue
                for (i in 0..2) {
                    val progress = ((t - i * 0.4f) / 1.2f).let { it - kotlin.math.floor(it) }
                    paint.alpha = (255 * opacity * smoothed * 0.6 * (1 - progress)).toInt().coerceIn(0, 255)
                    canvas.drawCircle(anchorX, anchorY, dp(60 + 90 * progress), paint)
                }
                val quiet = (t % 2f) / 1.6f
                if (quiet <= 1f) {
                    paint.alpha = (255 * opacity * 0.14 * max(0.0, 1 - smoothed * 3) * (1 - quiet)).toInt().coerceIn(0, 255)
                    canvas.drawCircle(anchorX, anchorY, dp(60 + quiet * 22), paint)
                }
            }
            val s = smoothed.coerceIn(0.0, 1.0)
            paint.style = Paint.Style.FILL; paint.color = Color.rgb((104 - 83 * s).toInt(), (139 - 91 * s).toInt(), (227 - 94 * s).toInt()); paint.alpha = (255 * opacity).toInt()
            canvas.drawCircle(anchorX, anchorY, dp(60f), paint)
            paint.style = Paint.Style.STROKE; paint.color = blue; paint.alpha = (178 * opacity).toInt(); paint.strokeWidth = dp(1.5f)
            canvas.drawCircle(anchorX, anchorY, dp(60f), paint)
            mic(canvas, anchorX, anchorY - dp(3f), dp(0.7f), Color.WHITE, opacity)
        }
        if (gesture || opacity > 0 || now - hintTime < 300) postInvalidateOnAnimation()
    }
}
