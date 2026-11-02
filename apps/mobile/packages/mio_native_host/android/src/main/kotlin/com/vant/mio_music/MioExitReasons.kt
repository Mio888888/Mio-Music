package com.vant.mio_music

import android.app.ActivityManager
import android.app.ApplicationExitInfo
import android.content.Context
import android.os.Build
import org.json.JSONArray
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.InputStream

/**
 * The system's own record of how earlier processes ended. Android TV shows no ANR dialog,
 * so every kind of ending looks like a crash to the viewer. Runs on the LAN server's thread.
 */
internal object MioExitReasons {
    private const val ENTRIES = 16
    private const val TRACE_BYTES = 4 * 1024 * 1024
    private const val DETAIL_CHARS = 12_000
    private const val FRAMES = 32

    fun read(context: Context): String {
        val activities = context.getSystemService(ActivityManager::class.java)
        val memory = ActivityManager.MemoryInfo().also { activities.getMemoryInfo(it) }
        val report = JSONObject()
            .put("device", "${Build.MANUFACTURER} ${Build.MODEL}")
            .put("sdk", Build.VERSION.SDK_INT)
            .put("abi", Build.SUPPORTED_ABIS.firstOrNull() ?: "")
            .put("memory", memory.totalMem)
        val exits = JSONArray()
        if (Build.VERSION.SDK_INT >= 30) {
            for (info in activities.getHistoricalProcessExitReasons(context.packageName, 0, ENTRIES)) {
                exits.put(JSONObject()
                    .put("time", info.timestamp)
                    .put("reason", reason(info.reason))
                    .put("status", info.status)
                    .put("importance", info.importance)
                    .put("pss", info.pss)
                    .put("rss", info.rss)
                    .put("description", info.description ?: "")
                    .put("detail", detail(info) ?: ""))
            }
        }
        return report.put("supported", Build.VERSION.SDK_INT >= 30).put("exits", exits).toString()
    }

    private fun reason(code: Int): String = when (code) {
        ApplicationExitInfo.REASON_ANR -> "anr"
        ApplicationExitInfo.REASON_CRASH -> "crash"
        ApplicationExitInfo.REASON_CRASH_NATIVE -> "crash_native"
        ApplicationExitInfo.REASON_DEPENDENCY_DIED -> "dependency_died"
        ApplicationExitInfo.REASON_EXCESSIVE_RESOURCE_USAGE -> "excessive_resource"
        ApplicationExitInfo.REASON_EXIT_SELF -> "exit_self"
        ApplicationExitInfo.REASON_FREEZER -> "freezer"
        ApplicationExitInfo.REASON_INITIALIZATION_FAILURE -> "initialization_failure"
        ApplicationExitInfo.REASON_LOW_MEMORY -> "low_memory"
        ApplicationExitInfo.REASON_OTHER -> "other"
        ApplicationExitInfo.REASON_PACKAGE_STATE_CHANGE -> "package_state"
        ApplicationExitInfo.REASON_PACKAGE_UPDATED -> "package_updated"
        ApplicationExitInfo.REASON_PERMISSION_CHANGE -> "permission_change"
        ApplicationExitInfo.REASON_SIGNALED -> "signaled"
        ApplicationExitInfo.REASON_USER_REQUESTED -> "user_requested"
        ApplicationExitInfo.REASON_USER_STOPPED -> "user_stopped"
        else -> "unknown"
    }

    private fun detail(info: ApplicationExitInfo): String? {
        val native = info.reason == ApplicationExitInfo.REASON_CRASH_NATIVE
        if (info.reason != ApplicationExitInfo.REASON_ANR && !native) return null
        return try {
            val bytes = info.traceInputStream?.use(::bounded) ?: return null
            when {
                // Before Android 12 the native trace is already text.
                native && Build.VERSION.SDK_INT >= 31 -> tombstone(bytes)
                native -> String(bytes)
                else -> anrMainThread(String(bytes))
            }.take(DETAIL_CHARS)
        } catch (_: Exception) { null }
    }

    private fun bounded(input: InputStream): ByteArray {
        val output = ByteArrayOutputStream()
        val buffer = ByteArray(64 * 1024)
        while (output.size() < TRACE_BYTES) {
            val count = input.read(buffer, 0, minOf(buffer.size, TRACE_BYTES - output.size()))
            if (count < 0) break
            output.write(buffer, 0, count)
        }
        return output.toByteArray()
    }

    /** A whole ANR trace runs to hundreds of KB; the main thread is what decided the ANR. */
    private fun anrMainThread(trace: String): String {
        val subject = trace.lineSequence().firstOrNull { it.startsWith("Subject:") }
        val start = trace.indexOf("\n\"main\" ")
        if (start < 0) return listOfNotNull(subject, trace).joinToString("\n")
        val end = trace.indexOf("\n\n", start + 1).takeIf { it > 0 } ?: trace.length
        return listOfNotNull(subject, trace.substring(start + 1, end)).joinToString("\n")
    }

    /** Field numbers come from AOSP's tombstone.proto; only the crashing thread is kept. */
    private fun tombstone(bytes: ByteArray): String {
        var crashed = 0L
        var signal = ""
        var abort = ""
        val threads = ArrayList<Proto>()
        Proto(bytes, 0, bytes.size).forEach { field, value, body ->
            when (field) {
                6 -> crashed = value
                10 -> body?.let { signal = signal(it) }
                14 -> abort = body?.text() ?: ""
                16 -> body?.let(threads::add)
            }
        }
        val lines = arrayListOf("signal $signal")
        if (abort.isNotEmpty()) lines += "abort $abort"
        for (entry in threads) {
            var id = -1L
            var thread: Proto? = null
            entry.forEach { field, value, body ->
                if (field == 1) id = value else if (field == 2) thread = body
            }
            if (id == crashed) thread?.let { lines += backtrace(it) }
        }
        return lines.joinToString("\n")
    }

    private fun signal(body: Proto): String {
        var name = ""
        var code = ""
        var address: Long? = null
        body.forEach { field, value, text ->
            when (field) {
                2 -> name = text?.text() ?: ""
                4 -> code = text?.text() ?: ""
                9 -> address = value
            }
        }
        return listOfNotNull(name, code, address?.let { "fault addr 0x${java.lang.Long.toHexString(it)}" }).joinToString(" ")
    }

    private fun backtrace(thread: Proto): List<String> {
        val lines = ArrayList<String>()
        var index = 0
        thread.forEach { field, _, body ->
            if (field == 2) lines += "thread ${body?.text() ?: ""}"
            if (field == 4 && body != null && index < FRAMES) lines += frame(index++, body)
        }
        return lines
    }

    private fun frame(index: Int, body: Proto): String {
        var pc = 0L
        var function = ""
        var offset = 0L
        var file = ""
        var build = ""
        body.forEach { field, value, text ->
            when (field) {
                1 -> pc = value
                4 -> function = text?.text() ?: ""
                5 -> offset = value
                6 -> file = text?.text() ?: ""
                8 -> build = text?.text() ?: ""
            }
        }
        val symbol = if (function.isEmpty()) "" else " ($function+$offset)"
        val id = if (build.isEmpty()) "" else " (BuildId: $build)"
        return "#%02d pc %016x %s%s%s".format(index, pc, file, symbol, id)
    }

    private class Proto(private val data: ByteArray, private var at: Int, private val end: Int) {
        private fun varint(): Long {
            var value = 0L
            var shift = 0
            while (true) {
                val byte = data[at++].toInt()
                value = value or ((byte and 0x7f).toLong() shl shift)
                if (byte >= 0) return value
                shift += 7
                require(shift < 70)
            }
        }
        fun forEach(visit: (Int, Long, Proto?) -> Unit) {
            while (at < end) {
                val key = varint()
                val field = (key ushr 3).toInt()
                when ((key and 7).toInt()) {
                    0 -> visit(field, varint(), null)
                    1 -> at += 8
                    2 -> {
                        val size = varint().toInt()
                        require(size in 0..(end - at))
                        visit(field, 0L, Proto(data, at, at + size))
                        at += size
                    }
                    5 -> at += 4
                    else -> throw IllegalArgumentException()
                }
            }
        }
        fun text() = String(data, at, end - at)
    }
}
