package com.vant.mio_music

import android.view.KeyEvent
import com.ryanheise.audioservice.AudioServiceActivity
import io.flutter.embedding.android.FlutterView
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodChannel

class MainActivity : AudioServiceActivity() {
    override fun shouldDestroyEngineWithHost(): Boolean = false

    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        val mediaKey = when (event.keyCode) {
            KeyEvent.KEYCODE_MEDIA_PLAY, KeyEvent.KEYCODE_MEDIA_PAUSE,
            KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE, KeyEvent.KEYCODE_MEDIA_STOP,
            KeyEvent.KEYCODE_MEDIA_NEXT, KeyEvent.KEYCODE_MEDIA_PREVIOUS,
            KeyEvent.KEYCODE_MEDIA_FAST_FORWARD, KeyEvent.KEYCODE_MEDIA_REWIND,
            KeyEvent.KEYCODE_HEADSETHOOK -> true
            else -> false
        }
        // Foreground Flutter focus must not swallow this session's media keys.
        if (mediaKey && NativeMedia.dispatchMediaButton(event)) return true
        return super.dispatchKeyEvent(event)
    }

    override fun onPostResume() {
        super.onPostResume()
        // The Dart focus tree cannot receive keys until its Android host owns
        // focus. Preserve a native editor/platform view when one already does.
        if (currentFocus == null) {
            findViewById<FlutterView>(FLUTTER_VIEW_ID)?.requestFocus()
        }
    }

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, "mio/host")
            .setMethodCallHandler { call, result ->
                when (call.method) {
                    "lyricOverlayState" -> result.success(LyricOverlay.state(this))
                    "requestLyricOverlayPermission" -> {
                        try {
                            if (isFinishing || isDestroyed) throw IllegalStateException()
                            startActivity(android.content.Intent(android.provider.Settings.ACTION_MANAGE_OVERLAY_PERMISSION,
                                android.net.Uri.parse("package:$packageName")))
                            result.success(null)
                        } catch (_: Exception) { result.error("permission_unavailable", "Overlay permission settings unavailable", null) }
                    }
                    "configureLyricOverlay" -> {
                        val enabled = call.argument<Boolean>("enabled")
                        val locked = call.argument<Boolean>("locked")
                        val scale = call.argument<Number>("scale")?.toFloat()
                        if (isFinishing || isDestroyed || enabled == null || locked == null || scale == null || !scale.isFinite() || scale !in .7f..2f ||
                            MioDevice.capabilities(this) and 1 != 0 || (enabled && !android.provider.Settings.canDrawOverlays(this))) {
                            result.error("overlay_unavailable", "Overlay permission is required on a phone or tablet", null)
                        } else {
                            LyricOverlay.preferences(this).edit().putBoolean("enabled", enabled).putBoolean("locked", locked).putFloat("scale", scale).apply()
                            NativeMedia.refreshOverlay()
                            result.success(LyricOverlay.state(this))
                        }
                    }
                    "audioOutputInfo" -> {
                        val manager = getSystemService(android.media.AudioManager::class.java)
                        val outputs = manager.getDevices(android.media.AudioManager.GET_DEVICES_OUTPUTS)
                        result.success(outputs.map { it.productName.toString() }.distinct().joinToString(" · "))
                    }
                    "openAudioOutputSettings" -> {
                        try { startActivity(android.content.Intent(android.provider.Settings.ACTION_SOUND_SETTINGS)); result.success(null) }
                        catch (_: Exception) { result.error("audio_settings_unavailable", "System audio settings unavailable", null) }
                    }
                    "appVersion" -> {
                        try {
                            @Suppress("DEPRECATION")
                            val version = packageManager.getPackageInfo(packageName, 0).versionName
                            if (version.isNullOrBlank()) result.error("invalid_version", "Package version unavailable", null)
                            else result.success(version)
                        } catch (_: Exception) {
                            result.error("invalid_version", "Package version unavailable", null)
                        }
                    }
                    "openAboutLink" -> {
                        val address = when (call.arguments as? String) {
                            "original" -> "https://github.com/timeshiftsauce/CeruMusic"
                            "docs" -> "https://ceru.docs.shiqianjiang.cn/"
                            "eqVault" -> "https://mio888888.github.io/EQVault/"
                            else -> null
                        }
                        if (address == null || isFinishing || isDestroyed) result.error("link_invalid", "About link unavailable", null)
                        else try {
                            startActivity(android.content.Intent(android.content.Intent.ACTION_VIEW, android.net.Uri.parse(address)).addCategory(android.content.Intent.CATEGORY_BROWSABLE))
                            result.success(null)
                        } catch (_: Exception) { result.error("open_failed", "No browser could open the link", null) }
                    }
                    "openBackupProvider" -> {
                        val url = when (call.arguments as? String) {
                            "hi168" -> "https://www.hi168.com"
                            "cstcloud" -> "https://data.cstcloud.cn/"
                            else -> null
                        }
                        if (url == null || isFinishing || isDestroyed) {
                            result.error("provider_link_invalid", "Provider page unavailable", null)
                        } else {
                            try {
                                startActivity(android.content.Intent(android.content.Intent.ACTION_VIEW, android.net.Uri.parse(url))
                                    .addCategory(android.content.Intent.CATEGORY_BROWSABLE))
                                result.success(null)
                            } catch (_: Exception) {
                                result.error("provider_open_failed", "No browser could open the provider page", null)
                            }
                        }
                    }
                    "openRelease" -> {
                        val tag = call.arguments as? String
                        if (tag == null || !tag.matches(Regex("[A-Za-z0-9.+-]{1,128}")) || isFinishing || isDestroyed) {
                            result.error("release_link_invalid", "Release page unavailable", null)
                        } else {
                            val uri = android.net.Uri.Builder().scheme("https").authority("github.com")
                                .appendPath("Mio888888").appendPath("Mio-Music")
                                .appendPath("releases").appendPath("tag").appendPath(tag).build()
                            try {
                                startActivity(android.content.Intent(android.content.Intent.ACTION_VIEW, uri)
                                    .addCategory(android.content.Intent.CATEGORY_BROWSABLE))
                                result.success(null)
                            } catch (_: Exception) {
                                result.error("release_open_failed", "No browser could open the release page", null)
                            }
                        }
                    }
                    "initializeAudioContext" -> {
                        try {
                            System.loadLibrary("mio_mobile_bridge")
                            NativeMedia.initialize(applicationContext)
                            if (NativeMedia.initializeContext(applicationContext, MioDevice.capabilities(applicationContext))) result.success(null)
                            else result.error("context_failed", "Audio context unavailable", null)
                        } catch (error: LinkageError) {
                            result.error("library_failed", "Native library unavailable", null)
                        }
                    }
                    else -> result.notImplemented()
                }
            }
    }
}
