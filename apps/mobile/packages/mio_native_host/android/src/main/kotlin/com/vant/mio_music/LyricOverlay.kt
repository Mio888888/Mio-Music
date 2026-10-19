package com.vant.mio_music

import android.content.Context
import android.graphics.Color
import android.graphics.PixelFormat
import android.graphics.drawable.GradientDrawable
import android.provider.Settings
import android.view.Gravity
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.view.View
import android.view.WindowManager
import android.widget.TextView
import org.json.JSONObject

/** One window owned by the existing playback service, with no second engine. */
class LyricOverlay(private val context: Context) {
    companion object {
        private const val PREFS = "mio_lyric_overlay"
        fun preferences(context: Context) = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        fun enabled(context: Context) = preferences(context).getBoolean("enabled", false) && Settings.canDrawOverlays(context)
        fun state(context: Context): Map<String, Any> {
            val p = preferences(context)
            return mapOf("supported" to !(MioDevice.capabilities(context) and 1 != 0), "permission" to Settings.canDrawOverlays(context),
                "enabled" to p.getBoolean("enabled", false), "locked" to p.getBoolean("locked", false),
                "scale" to p.getFloat("scale", 1f).toDouble())
        }
    }
    private val wm = context.getSystemService(WindowManager::class.java)
    private val prefs = preferences(context)
    private var view: TextView? = null
    private var params: WindowManager.LayoutParams? = null
    private var scale = prefs.getFloat("scale", 1f).coerceIn(.7f, 2f)
    private val density = context.resources.displayMetrics.density
    private var last: JSONObject? = null
    private fun dp(n: Float) = (n * density).toInt()
    fun refresh() { last?.let { update(it) } ?: close() }
    fun update(snapshot: JSONObject) {
        last = snapshot
        if (!enabled(context) || (MioDevice.capabilities(context) and 1 != 0) || snapshot.optJSONObject("current") == null || snapshot.optString("status") in listOf("idle", "failed", "ended")) { close(); return }
        val locked = prefs.getBoolean("locked", false)
        scale = prefs.getFloat("scale", 1f).coerceIn(.7f, 2f)
        val text = view ?: TextView(context).also { label ->
            label.gravity = Gravity.CENTER
            label.setPadding(dp(12f), dp(8f), dp(12f), dp(8f))
            label.maxLines = 3
            label.ellipsize = android.text.TextUtils.TruncateAt.END
            val layout = WindowManager.LayoutParams(dp(300f), WindowManager.LayoutParams.WRAP_CONTENT,
                WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL,
                PixelFormat.TRANSLUCENT)
            layout.gravity = Gravity.TOP or Gravity.LEFT
            layout.x = prefs.getInt("x", dp(12f)); layout.y = prefs.getInt("y", dp(120f))
            // Android's untrusted overlay opacity limit permits pass-through.
            layout.alpha = .75f
            val scaler = ScaleGestureDetector(context, object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
                override fun onScale(detector: ScaleGestureDetector): Boolean {
                    scale = (scale * detector.scaleFactor).coerceIn(.7f, 2f)
                    prefs.edit().putFloat("scale", scale).apply()
                    label.textSize = 20f * scale
                    layout.width = dp(300f * scale)
                    clamp(layout); updateLayout(label, layout)
                    return true
                }
            })
            var downX = 0f; var downY = 0f; var x = 0; var y = 0
            label.setOnTouchListener { _, event ->
                scaler.onTouchEvent(event)
                when (event.actionMasked) {
                    MotionEvent.ACTION_DOWN -> { downX = event.rawX; downY = event.rawY; x = layout.x; y = layout.y }
                    MotionEvent.ACTION_MOVE -> if (event.pointerCount == 1 && !scaler.isInProgress) {
                        layout.x = x + (event.rawX - downX).toInt(); layout.y = y + (event.rawY - downY).toInt()
                        clamp(layout); updateLayout(label, layout)
                    }
                    MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> prefs.edit().putInt("x", layout.x).putInt("y", layout.y).apply()
                }
                true
            }
            clamp(layout)
            try { wm.addView(label, layout); view = label; params = layout }
            catch (_: RuntimeException) { view = null; params = null }
        }
        if (view == null) return
        val layout = params ?: return
        layout.flags = WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE or WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL or
            (if (locked) WindowManager.LayoutParams.FLAG_NOT_TOUCHABLE else 0)
        layout.width = dp(300f * scale); clamp(layout)
        val dark = snapshot.optBoolean("dark", true)
        text.setTextColor(if (dark) Color.WHITE else Color.BLACK)
        text.background = GradientDrawable().apply { setColor(if (dark) Color.rgb(24,24,28) else Color.rgb(246,246,249)); cornerRadius = dp(12f).toFloat() }
        text.textSize = 20f * scale
        val song = snapshot.optJSONObject("current")
        text.text = snapshot.optString("lyric", "").takeIf { it.isNotBlank() && it != "null" }
            ?: song?.optString("title", "Mio Music") ?: "Mio Music"
        text.contentDescription = text.text
        updateLayout(text, layout)
    }
    private fun clamp(layout: WindowManager.LayoutParams) {
        val bounds = context.resources.displayMetrics
        layout.width = layout.width.coerceIn(dp(160f).coerceAtMost(bounds.widthPixels), bounds.widthPixels)
        layout.x = layout.x.coerceIn(0, (bounds.widthPixels - layout.width).coerceAtLeast(0))
        layout.y = layout.y.coerceIn(0, (bounds.heightPixels - dp(100f)).coerceAtLeast(0))
    }
    private fun updateLayout(view: View, layout: WindowManager.LayoutParams) {
        try { wm.updateViewLayout(view, layout) } catch (_: RuntimeException) { close() }
    }
    fun close() {
        val old = view; view = null; params = null
        if (old != null) try { wm.removeView(old) } catch (_: RuntimeException) { }
    }
}
