package dev.proflyder.home.feature.push

import android.app.KeyguardManager
import android.os.Build
import android.os.Bundle
import android.view.WindowManager
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import dev.proflyder.home.feature.push.impl.R
import dev.proflyder.home.feature.push.impl.databinding.ActivityFindPhoneBinding
import kotlinx.coroutines.launch

/**
 * The screen a full-screen intent opens: it turns the display on, shows over
 * the lock screen and offers one button, «Нашёл».
 *
 * Pressing it (or Back, or the notification action) stops the ring through the
 * same [FindPhoneController] the service uses. If the ring already stopped
 * (for example after the 60-second timeout) the screen closes itself, so the
 * owner never faces a stale alarm screen.
 */
class FindPhoneActivity : AppCompatActivity() {
    private lateinit var binding: ActivityFindPhoneBinding

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        binding = ActivityFindPhoneBinding.inflate(layoutInflater)
        setContentView(binding.root)
        showOverLockScreen()
        binding.findFound.setOnClickListener { stopAndFinish() }
        val controller = PushEntryPoint.require().findPhone
        binding.findReason.text = controller.ringing.value.reason
            .ifBlank { getString(R.string.push_find_default_reason) }
        lifecycleScope.launch {
            controller.ringing.collect { state ->
                if (!state.active) finish()
            }
        }
    }

    private fun stopAndFinish() {
        runCatching { PushEntryPoint.require().findPhone.stop() }
        finish()
    }

    private fun showOverLockScreen() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O_MR1) {
            setShowWhenLocked(true)
            setTurnScreenOn(true)
        } else {
            @Suppress("DEPRECATION")
            window.addFlags(
                WindowManager.LayoutParams.FLAG_SHOW_WHEN_LOCKED or
                    WindowManager.LayoutParams.FLAG_TURN_SCREEN_ON,
            )
        }
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        val keyguard = getSystemService(KeyguardManager::class.java)
        keyguard?.requestDismissKeyguard(this, null)
    }
}
