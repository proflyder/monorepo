package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.presentation.addBackCallback
import android.os.Bundle
import androidx.activity.OnBackPressedCallback
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.Notices
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.fabClearance
import dev.proflyder.home.core.ui.liftAboveBottomInsets
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.feature.control.impl.databinding.FragmentControlBinding

internal class ControlFragment :
    ScreenFragment<FragmentControlBinding, ControlUiState, ControlIntent, ControlEvent>(
        FragmentControlBinding.Companion::inflate,
    ) {
    override val viewModel: ControlViewModel by screenViewModel()

    private val dialogs = ControlDialogs(::requireContext, ::dispatch)
    private var list: ControlListRenderer? = null
    private var speedDial: SpeedDialOverlay? = null
    private val dialBack = object : OnBackPressedCallback(false) {
        override fun handleOnBackPressed() = dispatch(ControlIntent.Dial(open = false))
    }

    override fun onScreenCreated(binding: FragmentControlBinding, savedInstanceState: Bundle?) {
        binding.controlList.applyScrollBottomInset(binding.controlList.fabClearance())
        binding.controlQuickFab.liftAboveBottomInsets()
        binding.controlStatesBanner.liftAboveBottomInsets()
        list = ControlListRenderer(binding, ::dispatch)
        speedDial = SpeedDialOverlay(binding.controlRoot, binding.controlQuickFab, ::dispatch)
        binding.controlList.addOnScrollListener(FabOnScroll())
        addBackCallback(dialBack)
        binding.controlRefresh.setupPullToRefresh { dispatch(ControlIntent.Refresh) }
        binding.controlRetry.setOnClickListener { dispatch(ControlIntent.Refresh) }
        binding.controlOpenConnection.setOnClickListener { dispatch(ControlIntent.OpenConnection) }
    }

    override fun render(state: ControlUiState) {
        list?.render(state)
        speedDial?.render(state.speedDial, state.layout.dialOpen)
        dialBack.isEnabled = state.layout.dialOpen
    }

    override fun onEvent(event: ControlEvent) {
        when (event) {
            is ControlEvent.Notice -> Notices.show(binding.root, event.notice.text(requireContext()))
            is ControlEvent.Failure -> showFailure(event.failure)
            is ControlEvent.OpenLight -> dialogs.openLight(event.entry)
            is ControlEvent.OpenVolume -> dialogs.openVolume(event.entry, event.announce)
            is ControlEvent.OpenSpeak -> dialogs.openSpeak(event.entry)
            is ControlEvent.ConfirmPin -> dialogs.confirmPin(event.entry, event.pinned)
            is ControlEvent.AskAlarmDisable -> dialogs.askAlarmDisable(event.card)
            is ControlEvent.OpenRoute -> navigate(event.route)
        }
    }

    override fun onBindingDestroyed() {
        speedDial?.reset()
        speedDial = null
        list = null
        dialogs.dismissAll()
        dispatch(ControlIntent.Dial(open = false))
    }

    private inner class FabOnScroll : RecyclerView.OnScrollListener() {
        override fun onScrolled(recyclerView: RecyclerView, dx: Int, dy: Int) {
            if (viewModel.state.value.layout.dialOpen || dy == 0) return
            if (dy > 0) binding.controlQuickFab.hide() else binding.controlQuickFab.show()
        }
    }
}
