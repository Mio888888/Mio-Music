package com.vant.mio_music

import android.app.Activity
import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.webkit.MimeTypeMap
import androidx.core.content.FileProvider
import io.flutter.embedding.engine.plugins.FlutterPlugin
import io.flutter.embedding.engine.plugins.activity.ActivityAware
import io.flutter.embedding.engine.plugins.activity.ActivityPluginBinding
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import java.io.File
import java.nio.file.Files
import java.nio.file.LinkOption

class MioDownloadProvider : FileProvider()

/** No caller-supplied path or MIME type is accepted. Only a completed basename
 * selected in Rust can be presented, within the host's private Downloads root. */
class MioDownloadFiles : FlutterPlugin, ActivityAware, MethodChannel.MethodCallHandler {
    private var channel: MethodChannel? = null
    private var context: Context? = null
    private var activity: Activity? = null
    override fun onAttachedToEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        context = binding.applicationContext
        channel = MethodChannel(binding.binaryMessenger, "mio/downloads").also { it.setMethodCallHandler(this) }
    }
    override fun onDetachedFromEngine(binding: FlutterPlugin.FlutterPluginBinding) {
        channel?.setMethodCallHandler(null); channel = null; context = null; activity = null
    }
    override fun onAttachedToActivity(binding: ActivityPluginBinding) { activity = binding.activity }
    override fun onReattachedToActivityForConfigChanges(binding: ActivityPluginBinding) { activity = binding.activity }
    override fun onDetachedFromActivityForConfigChanges() { activity = null }
    override fun onDetachedFromActivity() { activity = null }

    override fun onMethodCall(call: MethodCall, result: MethodChannel.Result) {
        if (call.method != "openCompleted") { result.notImplemented(); return }
        val name = call.arguments as? String
        val host = activity
        val app = context
        if (name == null || name.isEmpty() || name.toByteArray(Charsets.UTF_8).size > 1024 ||
            name == "." || name == ".." || name.any { it.isISOControl() || it == '/' || it == '\\' || it == ':' }) {
            result.error("invalid_input", "Invalid download file", null); return
        }
        if (host == null || app == null || host.isFinishing || host.isDestroyed) {
            result.error("open_unavailable", "File opening unavailable", null); return
        }
        try {
            val root = File(app.getExternalFilesDir(null) ?: app.filesDir, "Downloads")
            val file = File(root, name)
            if (Files.isSymbolicLink(root.toPath()) || Files.isSymbolicLink(file.toPath()) ||
                !Files.isRegularFile(file.toPath(), LinkOption.NOFOLLOW_LINKS) || file.length() <= 0 ||
                file.canonicalFile.parentFile != root.canonicalFile) {
                result.error("file_unavailable", "Download file unavailable", null); return
            }
            val extension = file.extension.lowercase(java.util.Locale.ROOT)
            val mime = when (extension) {
                "ogg", "opus" -> "audio/ogg"
                "ape" -> "audio/x-ape"
                "alac", "m4a" -> "audio/mp4"
                else -> MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension)
            }
            if (mime == null || !mime.startsWith("audio/")) {
                result.error("unsupported_file", "Audio file type unsupported", null); return
            }
            val uri = FileProvider.getUriForFile(app, app.packageName + ".downloads", file)
            val view = Intent(Intent.ACTION_VIEW).apply {
                setDataAndType(uri, mime)
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                clipData = ClipData.newRawUri(name, uri)
            }
            if (view.resolveActivity(app.packageManager) == null) {
                result.error("no_handler", "No application can open this audio file", null); return
            }
            val title = if (app.resources.configuration.locales[0].language == "zh") "打开下载文件" else "Open downloaded file"
            host.startActivity(Intent.createChooser(view, title).apply {
                addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
            })
            result.success(null) // The chooser was launched, not proof of external playback.
        } catch (_: Exception) {
            result.error("open_failed", "Unable to open downloaded file", null)
        }
    }
}
