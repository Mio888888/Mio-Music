package com.vant.mio_music

import android.Manifest
import android.app.Activity
import android.app.Application
import android.content.Context
import android.content.pm.PackageManager
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import io.flutter.embedding.engine.plugins.FlutterPlugin
import io.flutter.embedding.engine.plugins.activity.ActivityAware
import io.flutter.embedding.engine.plugins.activity.ActivityPluginBinding
import io.flutter.plugin.common.MethodChannel
import io.flutter.plugin.common.PluginRegistry
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong

/** Permission belongs to one explicit foreground intent; Rust owns capture lifetime. */
class MioMicrophone : FlutterPlugin, ActivityAware,
    PluginRegistry.RequestPermissionsResultListener, Application.ActivityLifecycleCallbacks {
    companion object {
        private val hosts = AtomicLong(1)
        private val requests = AtomicInteger(0x6000)
        private val gate = Any()
        private var application: Context? = null
        private var owner = 0L
        private var foreground = false
        private var issued = 0L
        private var authorized = false
        private var active = 0L

        internal fun withPlayback(action: () -> Boolean): Boolean = synchronized(gate) {
            if (active != 0L) false else action()
        }
        internal fun withResume(token: Long, action: () -> Boolean): Boolean = synchronized(gate) {
            if (active != 0L || token != issued || !foreground || authorized) false else action()
        }
        @JvmStatic fun recording(): Boolean = synchronized(gate) { active != 0L }
        @JvmStatic fun acquire(token: Long): Boolean = synchronized(gate) {
            val app = application
            if (token <= 0 || active != 0L || token != issued || !authorized || !foreground ||
                app == null || app.checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) false
            else { authorized = false; active = token; true }
        }
        @JvmStatic fun finish(token: Long): Boolean = synchronized(gate) {
            if (active == token) { active = 0L; true } else active == 0L
        }
        private fun cancel(token: Long) {
            if (token > 0) try { NativeMedia.cancelRecognition(token) } catch (_: LinkageError) {}
        }
    }
    private val host = hosts.getAndIncrement()
    private val main = Handler(Looper.getMainLooper())
    private var app: Application? = null
    private var binding: ActivityPluginBinding? = null
    private var channel: MethodChannel? = null
    private data class Request(val token: Long, val code: Int, val result: MethodChannel.Result, var granted: Boolean = false)
    private var pending: Request? = null

    override fun onAttachedToEngine(engine: FlutterPlugin.FlutterPluginBinding) {
        app = engine.applicationContext as? Application
        synchronized(gate) { application = engine.applicationContext.applicationContext }
        app?.registerActivityLifecycleCallbacks(this)
        channel = MethodChannel(engine.binaryMessenger, "mio/microphone").also { channel ->
            channel.setMethodCallHandler { call, result ->
                when (call.method) {
                    "available" -> result.success(engine.applicationContext.packageManager.hasSystemFeature(PackageManager.FEATURE_MICROPHONE))
                    "request" -> request((call.arguments as? Number)?.toLong(), result)
                    "release" -> {
                        val token = (call.arguments as? Number)?.toLong()
                        if (token != null && token > 0) release(token)
                        result.success(null)
                    }
                    else -> result.notImplemented()
                }
            }
        }
    }
    private fun request(token: Long?, result: MethodChannel.Result) {
        val activity = binding?.activity
        val ready = synchronized(gate) { foreground && owner == host && issued == 0L && active == 0L }
        if (token == null || token <= 0 || activity == null || activity.isFinishing || !ready || pending != null) {
            result.error("microphone_busy", "Microphone unavailable in this view", null); return
        }
        if (!activity.packageManager.hasSystemFeature(PackageManager.FEATURE_MICROPHONE)) {
            result.error("microphone_unavailable", "No microphone available", null); return
        }
        if (activity.checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) {
            if (issue(token)) result.success(null) else result.error("microphone_busy", "Microphone unavailable", null)
            return
        }
        val code = requests.getAndIncrement()
        if (code > 0xffff) { result.error("microphone_unavailable", "Permission request capacity reached", null); return }
        val request = Request(token, code, result)
        pending = request
        main.postDelayed({ if (pending === request) failPending("microphone_timeout") }, 60_000)
        try { activity.requestPermissions(arrayOf(Manifest.permission.RECORD_AUDIO), code) }
        catch (_: RuntimeException) { failPending("microphone_unavailable") }
    }
    private fun issue(token: Long): Boolean = synchronized(gate) {
        if (!foreground || owner != host || issued != 0L || active != 0L) false
        else { issued = token; authorized = true; true }
    }
    override fun onRequestPermissionsResult(code: Int, permissions: Array<out String>, grants: IntArray): Boolean {
        val request = pending ?: return false
        if (request.code != code) return false
        val activity = binding?.activity
        if (activity == null || activity.checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            failPending("microphone_denied"); return true
        }
        request.granted = true
        deliverPermission()
        return true
    }
    private fun deliverPermission() {
        val request = pending ?: return
        if (!request.granted || !synchronized(gate) { foreground && owner == host }) return
        pending = null; main.removeCallbacksAndMessages(null)
        if (issue(request.token)) request.result.success(null)
        else request.result.error("microphone_busy", "Microphone unavailable", null)
    }
    private fun failPending(code: String) {
        val request = pending ?: return
        pending = null; main.removeCallbacksAndMessages(null)
        request.result.error(code, "Microphone permission unavailable", null)
    }
    private fun release(token: Long) {
        if (pending?.token == token) failPending("cancelled")
        synchronized(gate) {
            if (owner == host && issued == token) { issued = 0L; authorized = false; cancel(token) }
            // Keep active until the Rust input stream has dropped and finish runs.
        }
    }
    private fun revoke() {
        synchronized(gate) {
            if (owner == host) {
                // cancel serializes against the owner's short final resume gate.
                // It never waits for an owner command or a main-thread callback.
                cancel(if (issued != 0L) issued else active)
                foreground = false
                issued = 0L; authorized = false
            }
        }
    }
    override fun onActivityResumed(activity: Activity) {
        if (activity !== binding?.activity) return
        synchronized(gate) {
            if (owner != host) { cancel(if (issued != 0L) issued else active); issued = 0L; authorized = false }
            owner = host; foreground = true
        }
        deliverPermission()
    }
    override fun onActivityPaused(activity: Activity) { if (activity === binding?.activity) revoke() }
    override fun onActivityDestroyed(activity: Activity) { if (activity === binding?.activity) { revoke(); failPending("cancelled") } }
    override fun onActivityCreated(activity: Activity, state: Bundle?) {}
    override fun onActivityStarted(activity: Activity) {}
    override fun onActivityStopped(activity: Activity) {}
    override fun onActivitySaveInstanceState(activity: Activity, state: Bundle) {}
    override fun onAttachedToActivity(binding: ActivityPluginBinding) {
        this.binding = binding; binding.addRequestPermissionsResultListener(this)
        if (binding.activity.hasWindowFocus()) onActivityResumed(binding.activity)
    }
    private fun detach() {
        revoke(); failPending("cancelled")
        binding?.removeRequestPermissionsResultListener(this); binding = null
    }
    override fun onDetachedFromActivityForConfigChanges() = detach()
    override fun onReattachedToActivityForConfigChanges(binding: ActivityPluginBinding) = onAttachedToActivity(binding)
    override fun onDetachedFromActivity() = detach()
    override fun onDetachedFromEngine(engine: FlutterPlugin.FlutterPluginBinding) {
        detach(); app?.unregisterActivityLifecycleCallbacks(this); app = null
        channel?.setMethodCallHandler(null); channel = null
        main.removeCallbacksAndMessages(null)
    }
}
