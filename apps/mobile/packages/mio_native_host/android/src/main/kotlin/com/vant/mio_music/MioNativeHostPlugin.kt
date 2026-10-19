package com.vant.mio_music

import io.flutter.embedding.engine.plugins.FlutterPlugin
import io.flutter.embedding.engine.plugins.activity.ActivityAware
import io.flutter.embedding.engine.plugins.activity.ActivityPluginBinding
import io.flutter.plugin.common.MethodChannel

/** Registered automatically on every audio_service engine, including headless starts. */
class MioNativeHostPlugin : FlutterPlugin, ActivityAware {
    private val delegates: List<FlutterPlugin> = listOf(MioDevice(), MioMicrophone(), MioFilePicker(), MioDownloadFiles())
    private var channel: MethodChannel? = null
    private var owner: MioAudioHost? = null
    private var attached = false
    override fun onAttachedToEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        val app = binding.applicationContext
        attached = true
        delegates.forEach { it.onAttachedToEngine(binding) }
        val methods = MethodChannel(binding.binaryMessenger, "mio/system_media")
        channel = methods
        owner = NativeMedia.attach(app, methods)
        methods.setMethodCallHandler { call, result ->
            if (!attached) { result.error("host_closed", "Media host is closed", null); return@setMethodCallHandler }
            when (call.method) {
                "initialize" -> try {
                    System.loadLibrary("mio_mobile_bridge")
                    NativeMedia.initialize(app)
                    if (NativeMedia.initializeContext(app, MioDevice.capabilities(app))) {
                        owner?.initialized()
                        result.success(null)
                    }
                    else result.error("context_failed", "Audio context unavailable", null)
                } catch (_: LinkageError) { result.error("library_failed", "Native library unavailable", null) }
                "appVersion" -> try {
                    @Suppress("DEPRECATION") val version = app.packageManager.getPackageInfo(app.packageName, 0).versionName
                    if (version.isNullOrBlank()) result.error("invalid_version", "Package version unavailable", null) else result.success(version)
                } catch (_: Exception) { result.error("invalid_version", "Package version unavailable", null) }
                "command" -> {
                    val id = call.argument<Number>("runtimeId")?.toLong()
                    val command = call.argument<Number>("command")?.toInt()
                    val value = call.argument<Number>("value")?.toDouble() ?: 0.0
                    if (id == null || id <= 0 || command !in listOf(1,2,3,4,5,6,7,12) || !value.isFinite()) {
                        result.error("invalid_command", "Invalid media command", null)
                    } else NativeMedia.execute(id, command!!, value) { ok ->
                        if (ok) result.success(null) else result.error("media_command_failed", "Playback changed or command failed", null)
                    }
                }
                "failed" -> { owner?.failed(); result.success(null) }
                else -> result.notImplemented()
            }
        }
    }
    override fun onDetachedFromEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        attached = false
        owner?.let { NativeMedia.detach(it) }; owner = null
        channel?.setMethodCallHandler(null); channel = null
        delegates.reversed().forEach { it.onDetachedFromEngine(binding) }
    }
    override fun onAttachedToActivity(binding: ActivityPluginBinding) { delegates.filterIsInstance<ActivityAware>().forEach { it.onAttachedToActivity(binding) } }
    override fun onDetachedFromActivityForConfigChanges() { delegates.filterIsInstance<ActivityAware>().forEach { it.onDetachedFromActivityForConfigChanges() } }
    override fun onReattachedToActivityForConfigChanges(binding: ActivityPluginBinding) { delegates.filterIsInstance<ActivityAware>().forEach { it.onReattachedToActivityForConfigChanges(binding) } }
    override fun onDetachedFromActivity() { delegates.filterIsInstance<ActivityAware>().forEach { it.onDetachedFromActivity() } }
}
