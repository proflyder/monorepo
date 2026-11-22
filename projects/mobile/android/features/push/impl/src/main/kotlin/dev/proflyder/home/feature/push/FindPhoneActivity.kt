package dev.proflyder.home.feature.push

import android.app.KeyguardManager
import android.os.Build
import android.os.Bundle
import android.view.WindowManager
import androidx.activity.enableEdgeToEdge
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.lifecycleScope
import dev.proflyder.home.core.runtime.appGraph
import dev.proflyder.home.core.ui.applySystemBarsPadding
import dev.proflyder.home.feature.push.impl.R
import dev.proflyder.home.feature.push.impl.databinding.ActivityFindPhoneBinding
import dev.zacsweers.metro.Inject
import kotlinx.coroutines.launch

class FindPhoneActivity : AppCompatActivity() {
    private lateinit var binding: ActivityFindPhoneBinding

    @Inject
    internal lateinit var controller: FindPhoneController

    override fun onCreate(savedInstanceState: Bundle?) {
        appGraph<PushInjector>().inject(this)
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        binding = ActivityFindPhoneBinding.inflate(layoutInflater)
        setContentView(binding.root)
        binding.root.applySystemBarsPadding()
        showOverLockScreen()
        binding.findFound.setOnClickListener { stopAndFinish() }
        binding.findReason.text = controller.ringing.value.reason
            .ifBlank { getString(R.string.push_find_default_reason) }
        lifecycleScope.launch {
            controller.ringing.collect { state ->
                if (!state.active) finish()
            }
        }
    }

    private fun stopAndFinish() {
        controller.stop()
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
