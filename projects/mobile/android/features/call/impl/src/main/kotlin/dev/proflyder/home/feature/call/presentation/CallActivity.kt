package dev.proflyder.home.feature.call.presentation

import android.content.Intent
import android.os.Build
import android.os.Bundle
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import dev.proflyder.home.core.ui.applicationViewModelFactory
import dev.proflyder.home.core.ui.applySystemBarsPadding
import dev.proflyder.home.feature.call.impl.R
import dev.proflyder.home.feature.call.impl.databinding.ActivityCallBinding
import kotlinx.coroutines.launch

internal class CallActivity : AppCompatActivity() {

    private val viewModel: CallViewModel by viewModels { applicationViewModelFactory(CallViewModel::class.java) }
    private val texts by lazy { CallTexts(this) }
    private val routeSheet by lazy { CallRouteSheet(this, texts, viewModel::dispatch) }
    private val proximity by lazy { ProximityScreenLock(this) }
    private lateinit var binding: ActivityCallBinding
    private val screen by lazy { CallScreen(binding, texts) }

    private val permissionLauncher = registerForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { result ->
        viewModel.dispatch(CallIntent.PermissionsResult(result[android.Manifest.permission.RECORD_AUDIO] == true))
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        binding = ActivityCallBinding.inflate(layoutInflater)
        setContentView(binding.root)
        binding.root.applySystemBarsPadding()
        showOverLockScreen()
        lifecycle.addObserver(proximity)
        bindControls()
        observe()
        if (savedInstanceState == null) viewModel.dispatch(CallIntent.Open(intent.startsCall()))
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        if (intent.startsCall()) viewModel.dispatch(CallIntent.Open(startCall = true))
    }

    override fun onDestroy() {
        routeSheet.dismiss()
        super.onDestroy()
    }

    private fun showOverLockScreen() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O_MR1) return
        setShowWhenLocked(true)
        setTurnScreenOn(true)
    }

    private fun bindControls() {
        binding.callName.text = getString(R.string.call_station_name)
        binding.callMute.setOnClickListener { viewModel.dispatch(CallIntent.ToggleMute) }
        binding.callRoute.setOnClickListener { viewModel.dispatch(CallIntent.ShowRoutes) }
        binding.callHangup.setOnClickListener { viewModel.dispatch(CallIntent.HangupOrClose) }
        onBackPressedDispatcher.addCallback(this, backCallback)
    }

    private val backCallback = object : OnBackPressedCallback(true) {
        override fun handleOnBackPressed() = viewModel.dispatch(CallIntent.Back)
    }

    private fun observe() {
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                launch { viewModel.state.collect(::render) }
                launch { viewModel.events.collect(::onEvent) }
            }
        }
    }

    private fun render(state: CallScreenState) {
        screen.render(state)
        routeSheet.render(state.routeSheetVisible, state.routes)
        proximity.render(state.proximityLock)
    }

    private fun onEvent(event: CallEvent) {
        when (event) {
            is CallEvent.RequestPermissions -> permissionLauncher.launch(event.permissions.toTypedArray())
            CallEvent.Finish -> finish()
            CallEvent.MoveToBack -> moveTaskToBack(true)
        }
    }

    private fun Intent.startsCall(): Boolean = getBooleanExtra(EXTRA_START_CALL, false)

    companion object {
        const val EXTRA_START_CALL = "dev.proflyder.home.call.START_CALL"
    }
}
