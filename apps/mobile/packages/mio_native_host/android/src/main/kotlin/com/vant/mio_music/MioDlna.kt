package com.vant.mio_music

import android.content.Context
import android.content.pm.PackageManager
import android.net.wifi.WifiManager

/** Only Rust's bounded discovery worker owns this lock; never retain an Activity. */
object MioDlna {
    private val gate = Any()
    private var application: Context? = null
    private var active = 0L
    private var multicast: WifiManager.MulticastLock? = null
    private var releasePending = false

    fun initialize(context: Context) = synchronized(gate) {
        application = context.applicationContext
    }
    fun acquire(token: Long): Boolean = synchronized(gate) {
        if (token <= 0) return@synchronized false
        if (active != 0L && releasePending) {
            // The previous worker has joined. Retry its failed cleanup before admitting another.
            if (!release(active)) return@synchronized false
        }
        if (active != 0L) return@synchronized false
        val app = application ?: return@synchronized false
        if (!app.packageManager.hasSystemFeature(PackageManager.FEATURE_WIFI)) {
            // Ethernet-only TVs have no Wi-Fi multicast filter to unlock.
            active = token
            return@synchronized true
        }
        val manager = app.getSystemService(Context.WIFI_SERVICE) as? WifiManager
            ?: return@synchronized false
        var acquired: WifiManager.MulticastLock? = null
        try {
            acquired = manager.createMulticastLock("mio-dlna-discovery")
            acquired.setReferenceCounted(false)
            acquired.acquire()
            multicast = acquired; active = token
            true
        } catch (_: RuntimeException) {
            try { if (acquired?.isHeld == true) acquired.release() }
            catch (_: RuntimeException) { multicast = acquired; active = token; releasePending = true }
            false
        }
    }
    fun release(token: Long): Boolean = synchronized(gate) {
        if (token != active || token <= 0) return@synchronized false
        val held = multicast
        try {
            if (held?.isHeld == true) held.release()
            multicast = null; active = 0L; releasePending = false
            true
        } catch (_: RuntimeException) {
            // Keep ownership until cleanup succeeds; never replace a held lock.
            releasePending = true
            false
        }
    }
}
