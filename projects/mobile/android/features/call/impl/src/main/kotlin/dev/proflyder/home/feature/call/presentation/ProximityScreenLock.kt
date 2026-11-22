package dev.proflyder.home.feature.call.presentation

import android.annotation.SuppressLint
import android.content.Context
import android.os.PowerManager
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner

internal class ProximityScreenLock(private val context: Context) : DefaultLifecycleObserver {
    private var lock: PowerManager.WakeLock? = null

    fun render(enabled: Boolean) {
        if (enabled) acquire() else release()
    }

    override fun onStop(owner: LifecycleOwner) = release()

    @SuppressLint("WakelockTimeout")
    private fun acquire() {
        if (lock?.isHeld == true) return
        val power = context.getSystemService(PowerManager::class.java) ?: return
        lock = power.newWakeLock(PowerManager.PROXIMITY_SCREEN_OFF_WAKE_LOCK, TAG).apply {
            setReferenceCounted(false)
            acquire()
        }
    }

    private fun release() {
        lock?.takeIf { it.isHeld }?.release()
        lock = null
    }

    private companion object {
        const val TAG = "home:call"
    }
}
