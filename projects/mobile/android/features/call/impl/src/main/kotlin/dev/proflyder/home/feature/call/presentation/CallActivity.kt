package dev.proflyder.home.feature.call.presentation

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.os.PowerManager
import android.view.View
import androidx.activity.OnBackPressedCallback
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import com.google.android.material.bottomsheet.BottomSheetDialog
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.feature.call.AudioRouteOption
import dev.proflyder.home.feature.call.CallAudioRoute
import dev.proflyder.home.feature.call.CallAudioRouteState
import dev.proflyder.home.feature.call.CallEntryPoint
import dev.proflyder.home.feature.call.CallFailure
import dev.proflyder.home.feature.call.CallPhase
import dev.proflyder.home.feature.call.CallSnapshot
import dev.proflyder.home.feature.call.impl.R
import dev.proflyder.home.feature.call.impl.databinding.ActivityCallBinding
import dev.proflyder.home.feature.call.impl.databinding.SheetCallRoutesBinding
import java.util.Locale
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * Lock-screen call screen. It is a separate Activity (not a Fragment) because a
 * call must survive the main task being backgrounded or the device locked.
 */
class CallActivity : AppCompatActivity() {

    private lateinit var binding: ActivityCallBinding
    private lateinit var dependencies: CallEntryPoint.Dependencies
    private var timerJob: Job? = null
    private var proximityLock: PowerManager.WakeLock? = null
    private var routeSheet: BottomSheetDialog? = null
    private var pendingStart = false

    private val permissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) { result ->
        if (result[Manifest.permission.RECORD_AUDIO] == true || hasMicrophonePermission()) {
            dependencies.coordinator.startOutgoingCall()
        } else {
            dependencies.controller.reportPermissionDenied()
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        dependencies = CallEntryPoint.require()
        binding = ActivityCallBinding.inflate(layoutInflater)
        setContentView(binding.root)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O_MR1) {
            setShowWhenLocked(true)
            setTurnScreenOn(true)
        }

        binding.callMute.setOnClickListener {
            val snapshot = dependencies.controller.state.value
            dependencies.controller.setMuted(!snapshot.muted)
        }
        binding.callRoute.setOnClickListener { openRouteSheet() }
        binding.callHangup.setOnClickListener { onHangupOrClose() }
        onBackPressedDispatcher.addCallback(
            this,
            object : OnBackPressedCallback(true) {
                override fun handleOnBackPressed() {
                    if (dependencies.controller.state.value.isLive) moveTaskToBack(true) else finish()
                }
            },
        )

        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                launch { dependencies.controller.state.collect(::render) }
                launch { dependencies.audioController.state.collect(::renderRoutes) }
            }
        }

        if (savedInstanceState == null) {
            if (intent.getBooleanExtra(EXTRA_START_CALL, false)) {
                startCallWithPermissions()
            } else if (!dependencies.controller.state.value.isLive) {
                finish()
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (intent.getBooleanExtra(EXTRA_START_CALL, false)) {
            dependencies.controller.dismissEnded()
            pendingStart = true
            startCallWithPermissions()
        }
    }

    override fun onStop() {
        releaseProximity()
        super.onStop()
    }

    override fun onDestroy() {
        timerJob?.cancel()
        releaseProximity()
        dependencies.audioController.stop()
        routeSheet?.dismiss()
        routeSheet = null
        super.onDestroy()
    }

    private fun startCallWithPermissions() {
        pendingStart = true
        val missing = buildList {
            if (!hasMicrophonePermission()) add(Manifest.permission.RECORD_AUDIO)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S &&
                ContextCompat.checkSelfPermission(this@CallActivity, Manifest.permission.BLUETOOTH_CONNECT) !=
                PackageManager.PERMISSION_GRANTED
            ) {
                add(Manifest.permission.BLUETOOTH_CONNECT)
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
                ContextCompat.checkSelfPermission(this@CallActivity, Manifest.permission.POST_NOTIFICATIONS) !=
                PackageManager.PERMISSION_GRANTED
            ) {
                add(Manifest.permission.POST_NOTIFICATIONS)
            }
        }
        if (missing.isEmpty()) {
            dependencies.coordinator.startOutgoingCall()
        } else {
            permissionLauncher.launch(missing.toTypedArray())
        }
    }

    private fun hasMicrophonePermission(): Boolean =
        ContextCompat.checkSelfPermission(this, Manifest.permission.RECORD_AUDIO) ==
            PackageManager.PERMISSION_GRANTED

    private fun render(state: CallSnapshot) {
        binding.callName.text = getString(R.string.call_station_name)
        binding.callError.visibility = View.GONE

        if (state.phase != CallPhase.IDLE) pendingStart = false
        when (state.phase) {
            CallPhase.IDLE -> if (!pendingStart) finish()
            CallPhase.DIALING, CallPhase.RINGING, CallPhase.CONNECTING -> {
                binding.callStatus.text = getString(
                    when (state.phase) {
                        CallPhase.RINGING -> R.string.call_status_ringing
                        CallPhase.CONNECTING -> R.string.call_status_connecting
                        else -> R.string.call_status_dialing
                    },
                )
            }
            CallPhase.ACTIVE -> {
                startTimer(state)
                dependencies.audioController.start()
                acquireProximity()
            }
            CallPhase.ENDED -> {
                timerJob?.cancel()
                timerJob = null
                releaseProximity()
                dependencies.audioController.stop()
                binding.callStatus.text = endedText(state)
                binding.callError.apply {
                    text = state.failure?.let { failureText(it) }
                    visibility = if (state.failure == null) View.GONE else View.VISIBLE
                }
                binding.callHangup.text = getString(R.string.call_close)
            }
        }

        val live = state.isLive
        binding.callMute.isEnabled = live
        binding.callRoute.isEnabled = live
        binding.callMute.text = getString(if (state.muted) R.string.call_unmute else R.string.call_mute)
        binding.callMute.setIconResource(
            if (state.muted) R.drawable.call_ic_mic_off else R.drawable.call_ic_mic,
        )
        if (live) binding.callHangup.text = getString(R.string.call_hangup)
    }

    private fun startTimer(state: CallSnapshot) {
        timerJob?.cancel()
        val startedAt = state.activeSinceUnix * 1_000
        timerJob = lifecycleScope.launch {
            while (true) {
                val elapsed = ((System.currentTimeMillis() - startedAt).coerceAtLeast(0)) / 1_000
                binding.callStatus.text = getString(
                    R.string.call_status_active_timer,
                    String.format(Locale.ROOT, "%02d:%02d", elapsed / 60, elapsed % 60),
                )
                delay(1_000)
            }
        }
    }

    private fun endedText(state: CallSnapshot): String {
        val reason = when (state.endReason) {
            dev.proflyder.home.core.model.IntercomEndReason.HANGUP -> R.string.call_reason_hangup
            dev.proflyder.home.core.model.IntercomEndReason.DECLINED -> R.string.call_reason_declined
            dev.proflyder.home.core.model.IntercomEndReason.BUSY -> R.string.call_reason_busy
            dev.proflyder.home.core.model.IntercomEndReason.CANCELED -> R.string.call_reason_canceled
            dev.proflyder.home.core.model.IntercomEndReason.TIMEOUT -> R.string.call_reason_timeout
            dev.proflyder.home.core.model.IntercomEndReason.UNREACHABLE -> R.string.call_reason_unreachable
            dev.proflyder.home.core.model.IntercomEndReason.ERROR -> R.string.call_reason_error
            else -> R.string.call_reason_unknown
        }
        return getString(R.string.call_status_ended_reason, getString(reason))
    }

    private fun failureText(failure: CallFailure): String = getString(
        when (failure) {
            CallFailure.NotConfigured -> R.string.call_error_not_configured
            CallFailure.StationOffline -> R.string.call_error_station_offline
            CallFailure.Busy -> R.string.call_error_busy
            CallFailure.MediaFailure -> R.string.call_error_media
            CallFailure.Timeout -> R.string.call_error_timeout
            CallFailure.PermissionDenied -> R.string.call_error_permission
            is CallFailure.Remote -> R.string.call_error_remote
        },
    )

    private fun onHangupOrClose() {
        val snapshot = dependencies.controller.state.value
        if (snapshot.isLive) {
            dependencies.controller.hangup()
        } else {
            dependencies.controller.dismissEnded()
            finish()
        }
    }

    private fun renderRoutes(state: CallAudioRouteState) {
        val sheet = routeSheet ?: return
        populateRoutes(sheet, state)
    }

    private fun openRouteSheet() {
        val state = dependencies.audioController.state.value
        val sheet = BottomSheetDialog(this)
        val sheetBinding = SheetCallRoutesBinding.inflate(layoutInflater)
        sheet.setContentView(sheetBinding.root)
        routeSheet = sheet
        sheet.setOnDismissListener { routeSheet = null }
        populateRoutes(sheet, state)
        sheet.show()
    }

    private fun populateRoutes(sheet: BottomSheetDialog, state: CallAudioRouteState) {
        val container = (sheet.findViewById<View>(R.id.call_route_list) as? android.widget.LinearLayout) ?: return
        container.removeAllViews()
        state.available.forEach { option ->
            val selected = option.route == state.selected
            val button = MaterialButton(this).apply {
                text = if (selected) getString(R.string.call_route_selected, routeLabel(option)) else routeLabel(option)
                isAllCaps = false
                minHeight = resources.getDimensionPixelSize(R.dimen.call_route_min_height)
                setOnClickListener {
                    dependencies.audioController.select(option.route)
                    sheet.dismiss()
                }
            }
            container.addView(
                button,
                android.widget.LinearLayout.LayoutParams(
                    android.widget.LinearLayout.LayoutParams.MATCH_PARENT,
                    android.widget.LinearLayout.LayoutParams.WRAP_CONTENT,
                ).apply { topMargin = resources.getDimensionPixelSize(R.dimen.call_route_gap) },
            )
        }
    }

    private fun routeLabel(option: AudioRouteOption): String = when (option.route) {
        CallAudioRoute.EARPIECE -> getString(R.string.call_route_earpiece)
        CallAudioRoute.SPEAKER -> getString(R.string.call_route_speaker)
        CallAudioRoute.WIRED_HEADSET -> getString(R.string.call_route_wired)
        CallAudioRoute.BLUETOOTH -> option.label?.let {
            getString(R.string.call_route_bluetooth_named, it)
        } ?: getString(R.string.call_route_bluetooth)
    }

    @android.annotation.SuppressLint("WakelockTimeout")
    private fun acquireProximity() {
        if (proximityLock?.isHeld == true) return
        val power = getSystemService(PowerManager::class.java) ?: return
        @Suppress("DEPRECATION")
        val lock = power.newWakeLock(PowerManager.PROXIMITY_SCREEN_OFF_WAKE_LOCK, "home:call")
        lock.setReferenceCounted(false)
        lock.acquire()
        proximityLock = lock
    }

    private fun releaseProximity() {
        proximityLock?.let { if (it.isHeld) it.release() }
        proximityLock = null
    }

    companion object {
        const val EXTRA_START_CALL = "dev.proflyder.home.call.START_CALL"
    }
}
