package com.vant.mio_music

import android.app.ActivityManager
import android.content.*
import android.media.*
import android.os.*
import android.view.KeyEvent
import io.flutter.plugin.common.MethodChannel
import org.json.JSONObject
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

/** Focus, snapshot and lyric-window adapter. audio_service alone owns the FGS and MediaSession. */
internal class MioAudioHost(private val context: Context, private val channel: MethodChannel) {
    private val main = Handler(Looper.getMainLooper())
    private val observer = Executors.newSingleThreadScheduledExecutor()
    private val observationPending = AtomicBoolean(false)
    private val audio = context.getSystemService(AudioManager::class.java)
    private val overlay = LyricOverlay(context)
    private var request: AudioFocusRequest? = null
    @Volatile private var runtimeId = 0L
    @Volatile private var artworkRevision = 0L
    private var focusGeneration = 0L
    private var focused = false
    private var transientLoss = false
    @Volatile private var initialized = false
    private var destroyed = false
    private var preparingUntil = 0L
    private var newestRuntime = 0L
    private val noisy = object : BroadcastReceiver() {
        override fun onReceive(context: Context?, intent: Intent?) {
            if (intent?.action == AudioManager.ACTION_AUDIO_BECOMING_NOISY) {
                transientLoss = false; releaseFocus(); NativeMedia.send(runtimeId, 11)
            }
        }
    }
    init {
        if (Build.VERSION.SDK_INT >= 33) context.registerReceiver(noisy, IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY), Context.RECEIVER_NOT_EXPORTED)
        else @Suppress("DEPRECATION") context.registerReceiver(noisy, IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY))
        observer.scheduleWithFixedDelay({
            if (!initialized) return@scheduleWithFixedDelay
            if (!observationPending.compareAndSet(false, true)) return@scheduleWithFixedDelay
            var posted = false
            try {
                val observedId = runtimeId
                val text = try { NativeMedia.snapshot(artworkRevision, LyricOverlay.enabled(context)) } catch (_: Exception) { null } catch (_: LinkageError) { null }
                val snapshot = text?.let { try { JSONObject(it) } catch (_: Exception) { null } }
                posted = main.post {
                    if (destroyed) { observationPending.set(false); return@post }
                    if (snapshot == null) {
                        if (observedId != 0L && observedId == runtimeId) {
                            releaseFocus(); overlay.close(); runtimeId = 0; artworkRevision = 0
                            channel.invokeMethod("runtimeClosed", observedId)
                        }
                        observationPending.set(false)
                    } else {
                        val id = snapshot.optLong("runtime_id")
                        if (id < newestRuntime || id <= 0) { observationPending.set(false); return@post }
                        if (id != runtimeId) { releaseFocus(); artworkRevision = 0; runtimeId = id }
                        newestRuntime = id
                        overlay.update(snapshot)
                        val state = snapshot.optString("status")
                        val active = state == "playing" || state == "loading" || NativeMedia.dlnaHandoffActive(id)
                        if (!active && SystemClock.elapsedRealtime() >= preparingUntil && !transientLoss) releaseFocus()
                        // Fail closed if the sole system media owner disappears.
                        if (active && SystemClock.elapsedRealtime() >= preparingUntil && !isForeground()) {
                            releaseFocus(); NativeMedia.send(id, 9)
                        }
                        channel.invokeMethod("snapshot", text, object : MethodChannel.Result {
                            override fun success(result: Any?) {
                                if (!destroyed && runtimeId == id) artworkRevision = snapshot.optLong("artwork_revision")
                                observationPending.set(false)
                            }
                            override fun error(code: String, message: String?, details: Any?) { observationPending.set(false) }
                            override fun notImplemented() { observationPending.set(false) }
                        })
                    }
                }
            } finally { if (!posted) observationPending.set(false) }
        }, 250, 500, java.util.concurrent.TimeUnit.MILLISECONDS)
    }
    fun initialized() { initialized = true }
    fun refreshOverlay() = overlay.refresh()
    fun dispatchMediaButton(event: KeyEvent): Boolean {
        if (destroyed || runtimeId == 0L || !isForeground()) return false
        audio.dispatchMediaKeyEvent(event)
        return true
    }
    @Suppress("DEPRECATION")
    private fun isForeground(): Boolean = try {
        // getRunningServices is limited to our own services on Android O+. This
        // is an acknowledgement, not an inference from Dart's queued state.
        context.getSystemService(ActivityManager::class.java).getRunningServices(100).any {
            it.service.packageName == context.packageName &&
            it.service.className == "com.ryanheise.audioservice.AudioService" && it.foreground
        }
    } catch (_: RuntimeException) { false }
    fun prepare(id: Long, token: Long, capture: Long, expired: AtomicBoolean, complete: (Boolean) -> Unit) {
        if (destroyed || expired.get() || id < newestRuntime || MioMicrophone.recording()) { complete(false); return }
        val deadline = SystemClock.elapsedRealtime() + 2800
        preparingUntil = deadline + 700
        channel.invokeMethod("prepare", mapOf("runtimeId" to id, "token" to token), object : MethodChannel.Result {
            override fun success(result: Any?) {
                fun admit() {
                    if (destroyed || expired.get() || SystemClock.elapsedRealtime() >= deadline || id < newestRuntime) {
                        cancelPrepare(token); complete(false); return
                    }
                    if (!isForeground()) { main.postDelayed({ admit() }, 40); return }
                    val granted = if (capture == 0L) MioMicrophone.withPlayback { acquireFocus(id) }
                        else MioMicrophone.withResume(capture) { acquireFocus(id) }
                    if (!granted) cancelPrepare(token)
                    complete(granted)
                }
                if (result == true) admit() else { cancelPrepare(token); complete(false) }
            }
            override fun error(code: String, message: String?, details: Any?) { cancelPrepare(token); complete(false) }
            override fun notImplemented() { cancelPrepare(token); complete(false) }
        })
    }
    fun cancelPrepare(token: Long) { if (!destroyed) channel.invokeMethod("cancelPrepare", token) }
    fun retainDlnaFocus(id: Long): Boolean = MioMicrophone.withPlayback {
        if (destroyed || id != runtimeId || !focused || !isForeground() || !NativeMedia.dlnaHandoffActive(id)) false
        else { preparingUntil = SystemClock.elapsedRealtime() + 3500; true }
    }
    private fun acquireFocus(id: Long): Boolean {
        if (destroyed || id < newestRuntime || !isForeground()) return false
        if (runtimeId != id) { releaseFocus(); runtimeId = id; artworkRevision = 0 }
        newestRuntime = id
        return try {
            if (!focused) {
                val generation = ++focusGeneration
                val capturedId = id
                val focus = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
                    .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build())
                    .setWillPauseWhenDucked(true)
                    .setOnAudioFocusChangeListener({ change ->
                        if (!destroyed && generation == focusGeneration && capturedId == runtimeId) when (change) {
                            AudioManager.AUDIOFOCUS_GAIN -> { focused = true; transientLoss = false; NativeMedia.send(capturedId, 10) }
                            AudioManager.AUDIOFOCUS_LOSS_TRANSIENT, AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK -> { focused = false; transientLoss = true; NativeMedia.send(capturedId, 8) }
                            AudioManager.AUDIOFOCUS_LOSS -> { transientLoss = false; releaseFocus(); NativeMedia.send(capturedId, 9) }
                        }
                    }, main).build()
                request?.let { audio.abandonAudioFocusRequest(it) }; request = focus
                focused = audio.requestAudioFocus(focus) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
            }
            focused
        } catch (_: RuntimeException) { false }
    }
    fun failed() { releaseFocus(); NativeMedia.send(runtimeId, 9) }
    private fun releaseFocus() {
        ++focusGeneration; request?.let { audio.abandonAudioFocusRequest(it) }; request = null; focused = false
    }
    fun close() {
        destroyed = true; observer.shutdownNow(); main.removeCallbacksAndMessages(null)
        overlay.close(); releaseFocus(); context.unregisterReceiver(noisy)
        if (initialized && runtimeId > 0) NativeMedia.send(runtimeId, 9)
    }
}
