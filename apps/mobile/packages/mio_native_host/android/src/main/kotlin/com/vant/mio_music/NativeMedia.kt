package com.vant.mio_music

import android.content.Context
import android.os.Handler
import android.os.Looper
import androidx.annotation.Keep
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.CompletableFuture
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong
import io.flutter.plugin.common.MethodChannel
import android.view.KeyEvent

/** JNI control adapter retained by the audio_service engine, independently of the UI. */
// Rust invokes these callbacks by JNI name; R8 cannot discover those callers.
@Keep
object NativeMedia {
    @JvmStatic fun beginDlnaSearch(token: Long): Boolean = MioDlna.acquire(token)
    @JvmStatic fun endDlnaSearch(token: Long): Boolean = MioDlna.release(token)
    @JvmStatic external fun cancelRecognition(token: Long)
    @JvmStatic fun beginRecording(token: Long): Boolean = MioMicrophone.acquire(token)
    @JvmStatic fun endRecording(token: Long): Boolean = MioMicrophone.finish(token)
    @JvmStatic external fun dlnaHandoffActive(runtimeId: Long): Boolean
    /** A handoff may retain existing focus, never acquire it after cancellation. */
    @JvmStatic fun prepareDlnaResume(runtimeId: Long): Boolean {
        if (Looper.myLooper() == Looper.getMainLooper()) return false
        val expired = AtomicBoolean(false)
        val result = CompletableFuture<Boolean>()
        main.post {
            result.complete(!expired.get() && host?.retainDlnaFocus(runtimeId) == true)
        }
        return try { result.get(3, TimeUnit.SECONDS) }
        catch (_: Exception) { expired.set(true); false }
    }
    @JvmStatic external fun snapshot(seenArtwork: Long, lyricsEnabled: Boolean): String?
    @JvmStatic external fun command(runtimeId: Long, command: Int, value: Double): Boolean
    private val main = Handler(Looper.getMainLooper())
    private val commands = ThreadPoolExecutor(1, 1, 0L, TimeUnit.MILLISECONDS,
        ArrayBlockingQueue<Runnable>(8), ThreadPoolExecutor.AbortPolicy())
    @JvmStatic external fun initializeContext(application: Context, deviceFlags: Int): Boolean
    internal var host: MioAudioHost? = null
    private val requestIds = AtomicLong(1)
    fun initialize(application: Context) { MioDlna.initialize(application) }
    internal fun attach(application: Context, channel: MethodChannel): MioAudioHost {
        host?.close()
        return MioAudioHost(application.applicationContext, channel).also { host = it }
    }
    internal fun detach(owner: MioAudioHost) { if (host === owner) { host = null; owner.close() } }
    fun refreshOverlay() { host?.refreshOverlay() }
    fun dispatchMediaButton(event: KeyEvent): Boolean = host?.dispatchMediaButton(event) == true

    @JvmStatic fun preparePlayback(runtimeId: Long): Boolean = prepare(runtimeId, 0L)
    @JvmStatic fun prepareCaptureResume(runtimeId: Long, token: Long): Boolean = prepare(runtimeId, token)
    private fun prepare(runtimeId: Long, capture: Long): Boolean {
        if (Looper.myLooper() == Looper.getMainLooper() || MioMicrophone.recording()) return false
        val expired = AtomicBoolean(false)
        val result = CompletableFuture<Boolean>()
        val token = requestIds.getAndIncrement()
        main.post {
            val active = host
            if (expired.get() || active == null) result.complete(false)
            else active.prepare(runtimeId, token, capture, expired) { granted -> result.complete(granted && !expired.get()) }
        }
        return try { result.get(3, TimeUnit.SECONDS) }
        catch (_: Exception) {
            expired.set(true); main.post { host?.cancelPrepare(token) }; false
        }
    }
    internal fun execute(runtimeId: Long, command: Int, value: Double, reply: (Boolean) -> Unit) {
        try { commands.execute {
            val ok = try { NativeMedia.command(runtimeId, command, value) } catch (_: LinkageError) { false }
            main.post { reply(ok) }
        } } catch (_: java.util.concurrent.RejectedExecutionException) { reply(false) }
    }

    internal fun send(runtimeId: Long, command: Int, value: Double = 0.0) {
        if (command in 8..11) {
            try { NativeMedia.command(runtimeId, command, value) } catch (_: LinkageError) {}
            return
        }
        try { commands.execute { try { NativeMedia.command(runtimeId, command, value) } catch (_: LinkageError) {} } }
        catch (_: java.util.concurrent.RejectedExecutionException) { /* bounded admission */ }
    }
}
