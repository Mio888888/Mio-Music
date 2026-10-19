package com.vant.mio_music

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.os.CancellationSignal
import android.os.Handler
import android.os.Looper
import io.flutter.embedding.engine.plugins.FlutterPlugin
import io.flutter.embedding.engine.plugins.activity.ActivityAware
import io.flutter.embedding.engine.plugins.activity.ActivityPluginBinding
import io.flutter.plugin.common.MethodChannel
import io.flutter.plugin.common.PluginRegistry
import java.io.ByteArrayOutputStream
import java.io.InputStream
import java.io.OutputStream
import java.io.File
import java.io.FileOutputStream
import java.nio.file.Files
import java.util.UUID
import android.provider.OpenableColumns
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.SynchronousQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

/** One chooser/read per engine; no provider path or persistent URI grant escapes. */
class MioFilePicker : FlutterPlugin, ActivityAware, PluginRegistry.ActivityResultListener {
    companion object {
        private const val MAX_BYTES = 1024 * 1024
        // A result from an old engine must never match a new engine's chooser.
        private val sequence = AtomicInteger(0x4d00)
        private val processId = UUID.randomUUID().toString().replace("-", "")
        private val extensions = setOf("mp3", "flac", "wav", "ogg", "m4a", "aac", "wma", "opus", "ape", "alac")
        private const val OWNER = "mio-audio-staging-v1"
        private const val FILE_LIMIT = 256L * 1024 * 1024
        private const val BATCH_LIMIT = 512L * 1024 * 1024
    }
    private val main = Handler(Looper.getMainLooper())
    private val reader = ThreadPoolExecutor(1, 1, 0L, TimeUnit.MILLISECONDS,
        SynchronousQueue<Runnable>(), ThreadPoolExecutor.AbortPolicy())
    private var app: Context? = null
    private var binding: ActivityPluginBinding? = null
    private var channel: MethodChannel? = null
    private class Request(val code: Int, val token: Long, val audio: Boolean, var result: MethodChannel.Result?) {
        val cancelled = AtomicBoolean(false)
        val signal = CancellationSignal()
        val stream = AtomicReference<InputStream?>(null)
        val output = AtomicReference<OutputStream?>(null)
        var singleAudio = false
        var playlist = false
        var queue = false
        var cover = false
        var exportBytes: ByteArray? = null
        val stage = AtomicReference<File?>(null)
        var timeout: Runnable? = null // main-thread only
        fun cancel() {
            cancelled.set(true)
            try { signal.cancel() } catch (_: RuntimeException) {}
            try { stream.getAndSet(null)?.close() } catch (_: Exception) {}
            try { output.getAndSet(null)?.close() } catch (_: Exception) {}
        }
    }
    private val handedStages = mutableSetOf<File>() // main-thread only
    private var pending: Request? = null // main-thread only
    override fun onAttachedToEngine(engine: FlutterPlugin.FlutterPluginBinding) {
        app = engine.applicationContext
        channel = MethodChannel(engine.binaryMessenger, "mio/files").also { channel ->
            channel.setMethodCallHandler { call, result ->
                when (call.method) {
                    "pickPlugin", "pickEq" -> pick((call.arguments as? Number)?.toLong(), false, result)
                    "pickQueue" -> pick((call.arguments as? Number)?.toLong(), false, result, queue = true)
                    "pickCover" -> pick((call.arguments as? Number)?.toLong(), false, result, cover = true)
                    "pickPlaylist" -> pick((call.arguments as? Number)?.toLong(), false, result, playlist = true)
                    "pickAudio" -> pick((call.arguments as? Number)?.toLong(), true, result)
                    "pickRecognitionAudio" -> pick((call.arguments as? Number)?.toLong(), true, result, singleAudio = true)
                    "saveQueue" -> {
                        val args = call.arguments as? Map<*, *>
                        val token = (args?.get("token") as? Number)?.toLong()
                        val bytes = args?.get("bytes") as? ByteArray
                        val name = args?.get("name") as? String
                        if (bytes == null || bytes.isEmpty() || bytes.size > 64 * 1024 * 1024 || name == null ||
                            !Regex("cerumusic-playlist-[0-9]{4}-[0-9]{2}-[0-9]{2}\\.cmpl").matches(name)) result.error("invalid_input", "Invalid export", null)
                        else pick(token, false, result, bytes, queue = true, exportName = name)
                    }
                    "saveEq" -> {
                        val args = call.arguments as? Map<*, *>
                        val token = (args?.get("token") as? Number)?.toLong()
                        val bytes = args?.get("bytes") as? ByteArray
                        if (bytes == null || bytes.isEmpty() || bytes.size > 512 * 1024) result.error("invalid_input", "Invalid export", null)
                        else pick(token, false, result, bytes)
                    }
                    "discardAudio" -> {
                        val ticket = call.arguments as? String
                        val stage = handedStages.firstOrNull { it.name == ticket }
                        if (stage != null) { handedStages.remove(stage); removeStage(stage) }
                        result.success(null)
                    }
                    "cancel" -> {
                        if ((call.arguments as? Number)?.toLong() == pending?.token) cancelPending()
                        result.success(null)
                    }
                    else -> result.notImplemented()
                }
            }
        }
    }
    private fun pick(token: Long?, audio: Boolean, result: MethodChannel.Result, exportBytes: ByteArray? = null, playlist: Boolean = false, queue: Boolean = false, cover: Boolean = false, exportName: String = "mio-eq-config.json", singleAudio: Boolean = false) {
        if (token == null || token <= 0) { result.error("invalid_input", "Invalid picker request", null); return }
        val activity = binding?.activity
        if (activity == null || activity.isFinishing) {
            result.error("picker_unavailable", "File picker unavailable", null); return
        }
        if (pending != null || reader.activeCount != 0 || sequence.get() >= 0xffff) {
            result.error("picker_busy", "File picker busy", null); return
        }
        val request = Request(sequence.incrementAndGet(), token, audio, result)
        request.singleAudio = singleAudio
        request.exportBytes = exportBytes
        request.playlist = playlist
        request.queue = queue
        request.cover = cover
        pending = request
        try {
            @Suppress("DEPRECATION")
            activity.startActivityForResult(Intent(if (exportBytes == null) Intent.ACTION_OPEN_DOCUMENT else Intent.ACTION_CREATE_DOCUMENT).apply {
                addCategory(Intent.CATEGORY_OPENABLE)
                type = if (cover) "image/*" else if (exportBytes == null) "*/*" else if (queue) "application/octet-stream" else "application/json"
                if (exportBytes != null) putExtra(Intent.EXTRA_TITLE, exportName)
                if (audio) putExtra(Intent.EXTRA_ALLOW_MULTIPLE, !singleAudio)
                addFlags(if (exportBytes == null) Intent.FLAG_GRANT_READ_URI_PERMISSION else Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
            }, request.code)
        } catch (_: RuntimeException) {
            complete(request, error = "picker_unavailable")
        }
    }
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?): Boolean {
        val request = pending?.takeIf { it.code == requestCode } ?: return false
        if (resultCode != Activity.RESULT_OK) { complete(request); return true }
        if (request.exportBytes != null) { writeDocument(request, data); return true }
        if (request.audio) { readAudio(request, data); return true }
        val uri = data?.data
        val context = app
        if (uri == null || uri.scheme != "content" || context == null) {
            complete(request, error = "file_unreadable"); return true
        }
        val timeout = Runnable { if (pending === request) complete(request, error = "file_timeout") }
        request.timeout = timeout
        main.postDelayed(timeout, 30_000)
        try {
            reader.execute {
                var bytes: ByteArray? = null
                var name: String? = null
                var failure: String? = null
                try {
                    val limit = if (request.queue) 64 * 1024 * 1024 else if (request.playlist) 32 * 1024 * 1024 else if (request.cover) 5 * 1024 * 1024 else MAX_BYTES
                    if (request.playlist || request.queue || request.cover) {
                        name = context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null, request.signal)?.use { cursor ->
                            if (cursor.moveToFirst()) cursor.getString(0) else null
                        }
                        val selectedName = name
                        if (selectedName == null || selectedName.isEmpty() || selectedName.toByteArray(Charsets.UTF_8).size > 1024 ||
                            selectedName.any { it.isISOControl() || it == '/' || it == '\\' } ||
                            selectedName.substringAfterLast('.', "").lowercase(java.util.Locale.ROOT) !in (if (request.cover) setOf("png", "jpg", "jpeg", "gif", "webp") else if (request.queue) setOf("cpl", "cmpl") else setOf("json", "cpl", "cmpl"))) throw java.io.IOException()
                    }
                    context.contentResolver.openAssetFileDescriptor(uri, "r", request.signal).use { file ->
                        if (file == null) throw java.io.IOException()
                        if (file.length > limit) throw TooLarge()
                        file.createInputStream().use { input ->
                            request.stream.set(input)
                            val output = ByteArrayOutputStream()
                            val buffer = ByteArray(8192)
                            while (true) {
                                if (request.cancelled.get()) throw java.io.IOException()
                                val count = input.read(buffer, 0, minOf(buffer.size, limit + 1 - output.size()))
                                if (count < 0) break
                                if (count == 0) continue
                                output.write(buffer, 0, count)
                                if (output.size() > limit) throw TooLarge()
                            }
                            if (output.size() == 0) throw java.io.IOException()
                            bytes = output.toByteArray()
                        }
                    }
                } catch (_: TooLarge) { failure = "file_too_large" }
                catch (_: Exception) { failure = "file_unreadable" }
                finally { request.stream.set(null) }
                main.post {
                    main.removeCallbacks(timeout)
                    complete(request, if ((request.playlist || request.queue || request.cover) && bytes != null) mapOf("name" to name, "bytes" to bytes) else bytes, failure)
                }
            }
        } catch (_: java.util.concurrent.RejectedExecutionException) {
            main.removeCallbacks(timeout); complete(request, error = "picker_busy")
        }
        return true
    }
    private fun writeDocument(request: Request, data: Intent?) {
        val context = app
        val uri = data?.data
        val bytes = request.exportBytes
        if (context == null || uri?.scheme != "content" || bytes == null) {
            complete(request, error = "file_unwritable"); return
        }
        val timeout = Runnable { if (pending === request) complete(request, error = "file_timeout") }
        request.timeout = timeout; main.postDelayed(timeout, 30_000)
        try {
            reader.execute {
                var failure: String? = null
                try {
                    if (request.cancelled.get()) throw java.io.IOException()
                    context.contentResolver.openAssetFileDescriptor(uri, "wt", request.signal).use { file ->
                        if (file == null) throw java.io.IOException()
                        file.createOutputStream().use { output ->
                            request.output.set(output)
                            var offset = 0
                            while (offset < bytes.size) {
                                if (request.cancelled.get()) throw java.io.IOException()
                                val count = minOf(8192, bytes.size - offset)
                                output.write(bytes, offset, count); offset += count
                            }
                            output.flush()
                        }
                    }
                    if (request.cancelled.get()) throw java.io.IOException()
                } catch (_: Exception) { failure = "file_unwritable" }
                finally { request.output.set(null) }
                main.post { complete(request, if (failure == null) true else null, failure) }
            }
        } catch (_: java.util.concurrent.RejectedExecutionException) { complete(request, error = "picker_busy") }
    }
    private fun removeStage(stage: File) {
        try {
            val marker = File(stage, ".owner")
            if (Files.isSymbolicLink(stage.toPath()) || !stage.isDirectory ||
                Files.isSymbolicLink(marker.toPath()) || !marker.isFile || marker.length() > 64 || marker.readText() != OWNER) return
            val children = stage.listFiles() ?: return
            if (children.any { !it.isFile || Files.isSymbolicLink(it.toPath()) ||
                    !(it.name in setOf(".owner", "manifest.json") || it.name.matches(Regex("[0-9]{1,2}\\.[a-z0-9]{2,5}"))) }) return
            children.filter { it.name != ".owner" }.forEach { if (!it.delete()) return }
            File(stage, ".owner").delete(); stage.delete()
        } catch (_: Exception) {}
    }
    private fun readAudio(request: Request, data: Intent?) {
        val context = app
        val fileLimit = if (request.singleAudio) 128L * 1024 * 1024 else FILE_LIMIT
        if ((data?.clipData?.itemCount ?: 0) > (if (request.singleAudio) 1 else 100)) { complete(request, error = "file_too_large"); return }
        val uris = mutableListOf<android.net.Uri>()
        data?.clipData?.let { clip ->
            if (clip.itemCount <= 100) for (i in 0 until clip.itemCount) uris.add(clip.getItemAt(i).uri)
        }
        if (uris.isEmpty() && data?.data != null) uris.add(data.data!!)
        if (context == null || uris.isEmpty() || uris.size > (if (request.singleAudio) 1 else 100) || uris.any { it.scheme != "content" }) {
            complete(request, error = "file_unreadable"); return
        }
        val timeout = Runnable { if (pending === request) complete(request, error = "file_timeout") }
        request.timeout = timeout; main.postDelayed(timeout, 600_000)
        try {
            reader.execute {
                var ticket: String? = null
                var failure: String? = null
                try {
                    val root = File(context.cacheDir, "mio-audio-staging")
                    if (!root.mkdirs() && !root.isDirectory) throw java.io.IOException()
                    if (Files.isSymbolicLink(root.toPath())) throw java.io.IOException()
                    root.listFiles()?.filter { !it.name.startsWith(processId + "-") && it.name.matches(Regex("[0-9a-f]{32}-[0-9a-f]{32}")) }?.forEach(::removeStage)
                    val stage = File(root, processId + "-" + UUID.randomUUID().toString().replace("-", ""))
                    if (!stage.mkdir()) throw java.io.IOException()
                    request.stage.set(stage)
                    File(stage, ".owner").writeText(OWNER)
                    val files = JSONArray()
                    var total = 0L
                    for ((index, uri) in uris.distinct().withIndex()) {
                        if (request.cancelled.get()) throw java.io.IOException()
                        val rawName = context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null, request.signal)?.use { cursor ->
                            if (cursor.moveToFirst()) cursor.getString(0) else null
                        } ?: throw java.io.IOException()
                        if (rawName.length > 1024) throw java.io.IOException()
                        val extension = rawName.substringAfterLast('.', "").lowercase(java.util.Locale.ROOT)
                        if (extension !in extensions) throw java.io.IOException()
                        val stem = rawName.substringBeforeLast('.').map { if (it == '/' || it == '\\' || it.code < 32) '_' else it }.joinToString("").take(64).ifBlank { "audio" }
                        val name = "$stem.$extension"
                        val relative = "$index.$extension"
                        context.contentResolver.openAssetFileDescriptor(uri, "r", request.signal).use { source ->
                            if (source == null) throw java.io.IOException()
                            if (source.length > fileLimit) throw TooLarge()
                            source.createInputStream().use { input ->
                                request.stream.set(input)
                                FileOutputStream(File(stage, relative)).use { output ->
                                    val buffer = ByteArray(64 * 1024)
                                    var size = 0L
                                    while (true) {
                                        if (request.cancelled.get()) throw java.io.IOException()
                                        val read = input.read(buffer, 0, minOf(buffer.size.toLong(), fileLimit + 1 - size, BATCH_LIMIT + 1 - total).toInt())
                                        if (read < 0) break
                                        if (read == 0) continue
                                        size += read; total += read
                                        if (size > fileLimit || total > BATCH_LIMIT) throw TooLarge()
                                        output.write(buffer, 0, read)
                                    }
                                    if (size == 0L) throw java.io.IOException()
                                    output.fd.sync()
                                }
                            }
                        }
                        request.stream.set(null)
                        files.put(JSONObject().put("path", relative).put("name", name))
                    }
                    if (request.cancelled.get()) throw java.io.IOException()
                    val manifest = JSONObject().put("version", 1).put("files", files).toString().toByteArray(Charsets.UTF_8)
                    FileOutputStream(File(stage, "manifest.json")).use { it.write(manifest); it.fd.sync() }
                    ticket = stage.name
                } catch (_: TooLarge) { failure = "file_too_large" }
                catch (_: Exception) { failure = "file_unreadable" }
                finally { request.stream.set(null) }
                val stage = request.stage.get()
                if (failure != null || request.cancelled.get()) stage?.let(::removeStage)
                main.post { complete(request, ticket, failure) }
            }
        } catch (_: java.util.concurrent.RejectedExecutionException) { complete(request, error = "picker_busy") }
    }
    private class TooLarge : Exception()
    private fun complete(request: Request, bytes: Any? = null, error: String? = null) {
        if (pending !== request) { request.stage.get()?.let(::removeStage); return }
        if (bytes is String && error == null) request.stage.get()?.let { handedStages.add(it) }
        else if (request.audio && reader.activeCount == 0) request.stage.get()?.let(::removeStage)
        pending = null
        request.timeout?.let(main::removeCallbacks); request.timeout = null
        request.cancel()
        val result = request.result; request.result = null
        if (error == null) result?.success(bytes)
        else result?.error(error, "Selected files unavailable or exceed import limits", null)
    }
    private fun cancelPending() {
        val request = pending ?: return
        try { binding?.activity?.finishActivity(request.code) } catch (_: RuntimeException) {}
        complete(request)
    }
    override fun onAttachedToActivity(binding: ActivityPluginBinding) {
        this.binding = binding; binding.addActivityResultListener(this)
    }
    private fun detach() {
        cancelPending()
        binding?.removeActivityResultListener(this); binding = null
    }
    override fun onDetachedFromActivityForConfigChanges() = detach()
    override fun onReattachedToActivityForConfigChanges(binding: ActivityPluginBinding) = onAttachedToActivity(binding)
    override fun onDetachedFromActivity() = detach()
    override fun onDetachedFromEngine(engine: FlutterPlugin.FlutterPluginBinding) {
        detach()
        // Delivered stages may still be owned by Rust after engine disposal.
        // Rust drops its stage after work; abandoned tickets survive until the
        // next process's owned-stage cleanup, never interrupting a live reader.
        handedStages.clear()
        channel?.setMethodCallHandler(null); channel = null; app = null
        reader.shutdownNow(); main.removeCallbacksAndMessages(null)
    }
}
