package com.vant.Mio.Music

import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  companion object {
    init { System.loadLibrary("mio_lib") }
  }
  private external fun initAndroidContext(activity: android.app.Activity)

  override fun onCreate(savedInstanceState: Bundle?) {
    initAndroidContext(this)
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }
}
