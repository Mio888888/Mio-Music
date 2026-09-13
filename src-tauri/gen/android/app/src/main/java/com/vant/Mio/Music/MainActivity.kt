package com.vant.Mio.Music

import android.os.Bundle
import android.webkit.WebView
import android.view.ViewGroup.MarginLayoutParams
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.updateLayoutParams

class MainActivity : TauriActivity() {
  companion object {
    init { System.loadLibrary("mio_lib") }
  }
  private external fun initAndroidContext(activity: android.app.Activity)
  private var webView: WebView? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    initAndroidContext(this)
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  override fun onWebViewCreate(webView: WebView) {
    this.webView = webView
    // Size the actual WebView above system bars, including Android 9 three-button
    // navigation. Older WebViews cannot reliably expose CSS safe-area insets.
    ViewCompat.setOnApplyWindowInsetsListener(webView) { view, windowInsets ->
      val insets = windowInsets.getInsets(
        WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
      )
      view.updateLayoutParams<MarginLayoutParams> {
        leftMargin = insets.left
        topMargin = insets.top
        rightMargin = insets.right
        bottomMargin = insets.bottom
      }
      // Insets have already been applied natively; don't apply them again in CSS.
      WindowInsetsCompat.CONSUMED
    }
    ViewCompat.requestApplyInsets(webView)
  }

  override fun onPause() {
    super.onPause()
    if (MusicService.instance?.isPlaying() == true) {
      webView?.onResume()
    }
  }
}
