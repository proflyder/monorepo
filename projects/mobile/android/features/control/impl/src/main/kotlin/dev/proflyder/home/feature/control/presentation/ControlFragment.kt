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
import dev.proflyder.home.feature.alarms.UpcomingAlarmsSection
import dev.proflyder.home.feature.control.SpeedDialItem
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.FragmentControlBinding

internal class ControlFragment :
    ScreenFragment<FragmentControlBinding, ControlUiState, ControlIntent, ControlEvent>(
        FragmentControlBinding.Companion::inflate,
    ) {
    override val viewModel: ControlViewModel by screenViewModel()

    private val dialogs = ControlDialogs(::requireContext, ::dispatch)
    private val dialBack = object : OnBackPressedCallback(false) {
        override fun handleOnBackPressed() = dispatch(ControlIntent.Dial(open = false))
    }
    private var speedDial: SpeedDialOverlay? = null

    override fun onScreenCreate(savedInstanceState: Bundle?) {
        section(R.id.control_alarms_slot, UpcomingAlarmsSection)
    }

    override fun onViewReady(binding: FragmentControlBinding, savedInstanceState: Bundle?) {
        binding.controlList.applyScrollBottomInset(binding.controlList.fabClearance())
        binding.controlQuickFab.liftAboveBottomInsets()
        binding.controlStatesBanner.liftAboveBottomInsets()
        val list = ControlListRenderer(binding, ::dispatch)
        val dial = SpeedDialOverlay(binding.controlRoot, binding.controlQuickFab, ::dispatch).also { speedDial = it }
        binding.controlList.addOnScrollListener(FabOnScroll())
        addBackCallback(dialBack)
        wirePullToRefresh(binding) { dispatch(ControlIntent.Refresh) }
        binding.controlRetry.setOnClickListener { dispatch(ControlIntent.Refresh) }
        binding.controlOpenConnection.setOnClickListener { dispatch(ControlIntent.OpenConnection) }
        render(::controlListInputOf, list::render)
        render({ it.speedDial to it.layout.dialOpen }) { (items, open) -> renderDial(dial, items, open) }
    }

    private fun renderDial(dial: SpeedDialOverlay, items: List<SpeedDialItem>, open: Boolean) {
        dial.render(items, open)
        dialBack.isEnabled = open
    }

    override fun onEvent(event: ControlEvent) {
        when (event) {
            is ControlEvent.Notice -> Notices.show(binding.root, event.notice.text(requireContext()))
            is ControlEvent.Failure -> showFailure(event.failure)
            is ControlEvent.OpenLight -> dialogs.openLight(event.entry)
            is ControlEvent.OpenVolume -> dialogs.openVolume(event.entry, event.announce)
            is ControlEvent.OpenSpeak -> dialogs.openSpeak(event.entry)
            is ControlEvent.ConfirmPin -> dialogs.confirmPin(event.entry, event.pinned)
            is ControlEvent.OpenRoute -> navigate(event.route)
        }
    }

    override fun onBindingDestroyed() {
        speedDial?.reset()
        speedDial = null
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
