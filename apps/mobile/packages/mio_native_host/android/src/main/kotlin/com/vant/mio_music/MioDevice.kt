package com.vant.mio_music

import android.app.ActivityManager
import android.os.Process
import android.app.Activity
import android.app.Application
import android.os.Bundle
import android.app.UiModeManager
import android.content.Context
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.view.WindowManager
import io.flutter.embedding.engine.plugins.FlutterPlugin
import io.flutter.embedding.engine.plugins.activity.ActivityAware
import io.flutter.embedding.engine.plugins.activity.ActivityPluginBinding
import io.flutter.plugin.common.MethodChannel

/** Device shape is native; the window flag belongs only to this attached Activity. */
class MioDevice : FlutterPlugin, ActivityAware, Application.ActivityLifecycleCallbacks {
    companion object {
        fun capabilities(context: Context): Int {
            val pm = context.packageManager
            val television = context.getSystemService(UiModeManager::class.java)?.currentModeType == Configuration.UI_MODE_TYPE_TELEVISION ||
                pm.hasSystemFeature(PackageManager.FEATURE_LEANBACK)
            val limitedMemory = context.getSystemService(ActivityManager::class.java)?.isLowRamDevice == true || !Process.is64Bit()
            return (if (limitedMemory) 4 else 0) or (if (television) 1 else 0) or (if (pm.hasSystemFeature(PackageManager.FEATURE_MICROPHONE)) 2 else 0)
        }
    }
    private var channel: MethodChannel? = null
    private var activity: Activity? = null
    private var application: Application? = null
    private var epoch = 0L
    private var foreground = false
    private var owner: Long? = null
    private var television = false
    override fun onAttachedToEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        television = capabilities(binding.applicationContext) and 1 != 0
        application = binding.applicationContext as? Application
        application?.registerActivityLifecycleCallbacks(this)
        channel = MethodChannel(binding.binaryMessenger, "mio/device").also { channel ->
            channel.setMethodCallHandler { call, result ->
                if (call.method == "screenEpoch") {
                    result.success(if (television && foreground && activity != null) epoch else null)
                    return@setMethodCallHandler
                }
                if (call.method != "keepScreenOn") { result.notImplemented(); return@setMethodCallHandler }
                val token = call.argument<Number>("token")?.toLong()
                val active = call.argument<Boolean>("active")
                if (token == null || token <= 0 || active == null) {
                    result.error("invalid_input", "Invalid screen lease", null)
                } else {
                    val requestEpoch = call.argument<Number>("epoch")?.toLong()
                    if (active) {
                        if (!television || !foreground || activity == null || requestEpoch != epoch) {
                            result.error("activity_changed", "Screen lease requires the current foreground Activity", null)
                            return@setMethodCallHandler
                        }
                        owner = token
                    } else if (owner == token) owner = null
                    update()
                    result.success(null)
                }
            }
        }
    }
    private fun update() {
        val window = activity?.window ?: return
        if (television && foreground && owner != null) window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        else window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
    }
    private fun detach() {
        activity?.window?.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        owner = null; foreground = false; epoch++
        activity = null
    }
    override fun onDetachedFromEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        channel?.setMethodCallHandler(null); channel = null
        owner = null; detach()
        application?.unregisterActivityLifecycleCallbacks(this); application = null
    }
    override fun onAttachedToActivity(binding: ActivityPluginBinding) {
        detach(); activity = binding.activity
        // A cached engine may attach to an already resumed Activity.
        foreground = binding.activity.hasWindowFocus()
    }
    override fun onReattachedToActivityForConfigChanges(binding: ActivityPluginBinding) = onAttachedToActivity(binding)
    override fun onDetachedFromActivityForConfigChanges() = detach()
    override fun onDetachedFromActivity() = detach()
    override fun onActivityResumed(value: Activity) { if (activity === value) foreground = true }
    override fun onActivityPaused(value: Activity) { if (activity === value) { foreground = false; owner = null; epoch++; update() } }
    override fun onActivityStopped(value: Activity) { if (activity === value) { foreground = false; owner = null; epoch++; update() } }
    override fun onActivityDestroyed(value: Activity) { if (activity === value) detach() }
    override fun onActivityCreated(value: Activity, state: Bundle?) {}
    override fun onActivityStarted(value: Activity) {}
    override fun onActivitySaveInstanceState(value: Activity, state: Bundle) {}
}
