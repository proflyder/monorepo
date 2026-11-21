package dev.proflyder.home.feature.control.presentation

import android.os.Bundle
import androidx.activity.OnBackPressedCallback
import androidx.fragment.app.activityViewModels
import androidx.recyclerview.widget.GridLayoutManager
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.FavoriteKind
import dev.proflyder.home.core.model.FavoriteRef
import dev.proflyder.home.core.navigation.requireNavigator
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.ListStateViews
import dev.proflyder.home.core.ui.Notices
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.fabClearance
import dev.proflyder.home.core.ui.liftAboveBottomInsets
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.alarms.AlarmDisableChoice
import dev.proflyder.home.feature.alarms.alarmDisableChoice
import dev.proflyder.home.feature.control.ControlFilter
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.ControlItems
import dev.proflyder.home.feature.control.ControlTexts
import dev.proflyder.home.feature.control.HubCard
import dev.proflyder.home.feature.control.QuickTile
import dev.proflyder.home.feature.control.QuickTiles
import dev.proflyder.home.feature.control.SpeedDialItem
import dev.proflyder.home.feature.control.SpeedDialItems
import dev.proflyder.home.feature.control.WakeFeedback
import dev.proflyder.home.feature.control.WakeFeedbackAction
import dev.proflyder.home.feature.control.WakeFeedbackKind
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.FragmentControlBinding
import kotlinx.coroutines.delay

class ControlFragment : BaseFragment<FragmentControlBinding>(FragmentControlBinding.Companion::inflate) {
    internal val viewModel: ControlViewModel by activityViewModels {
        viewModelFactory(ControlViewModel::class.java)
    }

    private lateinit var adapter: ControlAdapter
    private lateinit var states: ListStateViews
    private lateinit var speedDial: SpeedDialOverlay
    private lateinit var dialBack: OnBackPressedCallback
    private var lightSheet: LightSheet? = null
    private var volumeSheet: VolumeSheet? = null
    private var speakDialog: SpeakDialog? = null

    private var hubWasEmpty = true

    private val adapterCallback = object : BaseAdapterCallback<ControlItem>(), ControlAdapterCallback {
        override fun onItemClick(item: ControlItem) {
            when (item) {
                is ControlItem.Scenario ->
                    if (item.entry.kind == ControlKind.ACTION) openSpeakDialog(item.entry)
                    else viewModel.run(item.entry)
                is ControlItem.Device ->
                    when {
                        item.entry.isVolume && item.entry.available -> openVolumeSheet(item.entry)
                        item.entry.light != null && item.entry.available -> openLightSheet(item.entry)
                        else -> viewModel.setPower(item.entry, !item.entry.isOn())
                    }
                is ControlItem.Header, is ControlItem.Voice, is ControlItem.DeviceGroup,
                is ControlItem.Hub, is ControlItem.QuickActions,
                is ControlItem.Filters,
                -> Unit
            }
        }

        override fun onItemLongClick(item: ControlItem) {
            if (item !is ControlItem.Device) return
            if (item.entry.light != null && item.entry.available) {
                openLightSheet(item.entry)
            } else {
                showPinDialog(item.entry)
            }
        }

        override fun onPowerChanged(entry: ControlEntry, on: Boolean) = viewModel.setPower(entry, on)

        override fun onButtonPressed(entry: ControlEntry) = viewModel.press(entry)

        override fun onProfileChanged(entry: ControlEntry, work: Boolean) = viewModel.setProfile(entry, work)

        override fun onToggleSettings(deviceId: String) = viewModel.toggleDeviceSettings(deviceId)

        override fun onHeaderClick(header: ControlItem.Header) {
            if (header.collapsible) viewModel.toggleRoom(header.section)
        }

        override fun onPinToggle(entry: ControlEntry) = showPinDialog(entry)

        override fun onHubCardClick(card: HubCard) = openHubSection(card)

        override fun onHubPrimary(card: HubCard) {
            when {
                card is HubCard.Alarm && hubAlarmNeedsChoice(card) -> showHubAlarmDisableChoice(card)
                else -> viewModel.hubPrimary(card)
            }
        }

        override fun onQuickTileClick(tile: QuickTile) {
            when (tile) {
                is QuickTile.Device ->
                    when {
                        tile.entry.isVolume && tile.entry.available -> openVolumeSheet(tile.entry)
                        tile.entry.light != null && tile.entry.available -> openLightSheet(tile.entry)
                        else -> viewModel.setPower(tile.entry, !tile.entry.isOn())
                    }
                is QuickTile.Speak -> openSpeakDialog(tile.entry)
                is QuickTile.ScenarioTile -> viewModel.runScenarioNow(tile.scenario)
                QuickTile.Call -> viewModel.callStation()
                QuickTile.Vpn -> openVpn()
            }
        }

        override fun onQuickTileLongClick(tile: QuickTile) {
            when (tile) {
                is QuickTile.Device ->
                    when {
                        tile.entry.isVolume -> openVolumeSheet(tile.entry)
                        tile.entry.light != null -> openLightSheet(tile.entry)
                        else -> viewModel.toggleFavoriteRef(
                            FavoriteRef(FavoriteKind.DEVICE, tile.entry.id),
                            tile.entry.localizedDisplayName(requireContext()),
                        )
                    }
                is QuickTile.Speak ->
                    viewModel.toggleFavoriteRef(
                        FavoriteRef(FavoriteKind.ACTION, tile.entry.id),
                        tile.entry.localizedDisplayName(requireContext()),
                    )
                is QuickTile.ScenarioTile ->
                    viewModel.toggleFavoriteRef(
                        FavoriteRef(FavoriteKind.SCENARIO, tile.scenario.id),
                        tile.scenario.label,
                    )
                QuickTile.Call, QuickTile.Vpn -> Unit
            }
        }

        override fun onFilterSelected(filter: ControlFilter) = viewModel.setFilter(filter)

        override fun onWakeAction(action: WakeFeedbackAction) = viewModel.markWake(action)
    }

    override fun onBindingCreated(binding: FragmentControlBinding, savedInstanceState: Bundle?) {
        binding.controlList.applyScrollBottomInset(binding.controlList.fabClearance())
        binding.controlQuickFab.liftAboveBottomInsets()
        binding.controlStatesBanner.liftAboveBottomInsets()
        adapter = ControlAdapter(adapterCallback)
        val spanCount = resources.getInteger(R.integer.control_columns)
        val deviceSpan = spanCount / resources.getInteger(R.integer.control_device_columns)
        binding.controlList.layoutManager = GridLayoutManager(requireContext(), spanCount).apply {
            spanSizeLookup = object : GridLayoutManager.SpanSizeLookup() {
                override fun getSpanSize(position: Int): Int = when (adapter.currentList.getOrNull(position)) {
                    is ControlItem.Scenario -> 1
                    is ControlItem.Device, is ControlItem.DeviceGroup -> deviceSpan
                    else -> spanCount
                }
            }
        }
        binding.controlList.adapter = adapter
        binding.controlList.addOnScrollListener(object : RecyclerView.OnScrollListener() {
            override fun onScrolled(recyclerView: RecyclerView, dx: Int, dy: Int) {
                if (speedDial.isOpen || dy == 0) return
                when {
                    dy > 0 -> binding.controlQuickFab.hide()
                    else -> binding.controlQuickFab.show()
                }
            }
        })

        speedDial = SpeedDialOverlay(
            anchor = binding.controlRoot,
            fab = binding.controlQuickFab,
            onClick = { item -> runSpeedDialItem(item) },
            onLongClick = { item -> pinSpeedDialItem(item) },
            onOpenChanged = { open -> dialBack.isEnabled = open },
        )
        dialBack = object : OnBackPressedCallback(false) {
            override fun handleOnBackPressed() = speedDial.close()
        }
        requireActivity().onBackPressedDispatcher.addCallback(viewLifecycleOwner, dialBack)

        states = ListStateViews(
            progress = binding.controlProgress,
            empty = binding.controlEmpty,
            error = binding.controlError,
            content = binding.controlContent,
            errorText = binding.controlErrorText,
        )
        binding.controlRefresh.setupPullToRefresh { viewModel.refresh() }
        binding.controlRetry.setOnClickListener { viewModel.refresh() }
        binding.controlOpenConnection.setOnClickListener { requireNavigator().selectTab("connection") }

        viewModel.state.observe { render(it) }

        repeatOnView {
            while (true) {
                delay(1_000)
                viewModel.tick()
            }
        }
    }

    internal fun openLightSheet(entry: ControlEntry) {
        lightSheet?.dismiss()
        lightSheet = LightSheet(requireContext(), entry) { adjust -> viewModel.adjust(entry, adjust) }
            .also { it.show() }
    }

    internal fun openVolumeSheet(entry: ControlEntry) {
        volumeSheet?.dismiss()
        val announce = entry.volumeCompanion
            ?: viewModel.state.value.entries.firstOrNull {
                it.isVolume && it.deviceId == entry.deviceId && it.id != entry.id
            }
        volumeSheet = VolumeSheet(
            context = requireContext(),
            entry = entry,
            announceEntry = announce,
            onLevel = { target, level -> viewModel.setVolume(target, level) },
            onMuted = { target, muted -> viewModel.setVolumeMuted(target, muted) },
        ).also { it.show() }
    }

    internal fun openSpeakDialog(entry: ControlEntry) {
        speakDialog?.dismiss()
        speakDialog = SpeakDialog(requireContext()) { text -> viewModel.speak(entry, text) }
            .also { it.show() }
    }

    private fun runSpeedDialItem(item: SpeedDialItem) {
        when (item) {
            is SpeedDialItem.Tile -> runQuickTileFromDial(item.tile)
            is SpeedDialItem.Wake -> viewModel.markWake(item.action)
        }
    }

    private fun runQuickTileFromDial(tile: QuickTile) {
        when (tile) {
            is QuickTile.Device -> viewModel.setPower(tile.entry, !tile.entry.isOn())
            is QuickTile.Speak -> openSpeakDialog(tile.entry)
            is QuickTile.ScenarioTile -> viewModel.runScenarioNow(tile.scenario)
            QuickTile.Call -> viewModel.callStation()
            QuickTile.Vpn -> openVpn()
        }
    }

    private fun pinSpeedDialItem(item: SpeedDialItem) {
        when (item) {
            is SpeedDialItem.Tile -> pinQuickTile(item.tile)
            is SpeedDialItem.Wake -> Unit
        }
    }

    private fun pinQuickTile(tile: QuickTile) {
        when (tile) {
            is QuickTile.Device -> viewModel.toggleFavoriteRef(
                FavoriteRef(FavoriteKind.DEVICE, tile.entry.id),
                tile.entry.localizedDisplayName(requireContext()),
            )
            is QuickTile.Speak -> viewModel.toggleFavoriteRef(
                FavoriteRef(FavoriteKind.ACTION, tile.entry.id),
                tile.entry.localizedDisplayName(requireContext()),
            )
            is QuickTile.ScenarioTile -> viewModel.toggleFavoriteRef(
                FavoriteRef(FavoriteKind.SCENARIO, tile.scenario.id),
                tile.scenario.label,
            )
            QuickTile.Call, QuickTile.Vpn -> Unit
        }
    }

    override fun onBindingDestroyed() {
        if (::speedDial.isInitialized) speedDial.reset()
        lightSheet?.dismiss()
        lightSheet = null
        volumeSheet?.dismiss()
        volumeSheet = null
        speakDialog?.dismiss()
        speakDialog = null
    }

    override fun onResume() {
        super.onResume()
        viewModel.refresh(silent = true)
    }

    private fun render(state: ControlUiState) {
        val hasItems = state.entries.isNotEmpty()
        val loadError = state.loadError
        when {
            !state.configured -> {
                states.showError(getString(R.string.control_not_configured))
                binding.controlRetry.setVisible(false)
                binding.controlOpenConnection.setVisible(true)
            }
            state.refreshing && !hasItems -> states.showLoading()
            loadError != null && !hasItems -> {
                states.showError(loadError.userMessage(requireContext()))
                binding.controlRetry.setVisible(true)
                binding.controlOpenConnection.setVisible(false)
            }
            !hasItems -> states.showEmpty()
            else -> {
                states.showContent()
                val tiles = QuickTiles.build(
                    refs = state.favorites,
                    entries = state.entries,
                    scenarios = state.scenarios,
                    pendingEntries = state.pending,
                    pendingScenarios = state.pendingScenarios,
                )
                val wake = WakeFeedback.actions(state.entries, state.pending)
                speedDial.render(SpeedDialItems.build(tiles, wake))
                val leading = listOf(
                    ControlItem.Hub(state.hubCards, state.hubRemaining, state.nowUnix),
                )
                val stickToTop = hubWasEmpty &&
                    state.hubCards.isNotEmpty() &&
                    !binding.controlList.canScrollVertically(-1)
                hubWasEmpty = state.hubCards.isEmpty()
                val items = ControlItems.build(
                    entries = state.entries,
                    pending = state.pending,
                    statesOk = state.statesOk,
                    filter = state.filter,
                    texts = texts(),
                    expandedDevices = state.expandedDevices,
                    leading = leading,
                    collapsedRooms = state.collapsedRooms,
                    voiceCard = false,
                )
                adapter.submitList(items) { if (stickToTop) binding.controlList.scrollToPosition(0) }
            }
        }
        binding.controlRefresh.isRefreshing = state.refreshing && hasItems
        binding.controlStatesBanner.setVisible(hasItems && !state.statesOk)

        if (loadError != null && hasItems) {
            showFailure(loadError)
            viewModel.consumeLoadError()
        }
        state.actionError?.let { failure ->
            showFailure(failure)
            viewModel.consumeActionError()
        }
        state.notice?.let { notice ->
            showNotice(notice)
            viewModel.consumeNotice()
        }
    }

    private fun showNotice(notice: ControlNotice) {
        when (notice) {
            is ControlNotice.ScenarioQueued ->
                Notices.show(binding.root, getString(R.string.control_scenario_queued, notice.title))
            is ControlNotice.FavoriteChanged ->
                Notices.show(
                    binding.root,
                    getString(
                        if (notice.pinned) R.string.control_favorite_pinned else R.string.control_favorite_unpinned,
                        notice.title,
                    ),
                )
            is ControlNotice.WakeMarked ->
                Notices.show(
                    binding.root,
                    getString(
                        when (notice.kind) {
                            WakeFeedbackKind.FALSE_ALARM -> R.string.control_wake_false_alarm_sent
                            WakeFeedbackKind.MISSED -> R.string.control_wake_missed_sent
                        },
                    ),
                )
            ControlNotice.CommandSent ->
                Notices.show(binding.root, getString(R.string.control_command_sent))
        }
    }

    internal fun showPinDialog(entry: ControlEntry) {
        val ref = FavoriteRef(FavoriteKind.DEVICE, entry.id)
        val pinned = ref in viewModel.state.value.favorites
        confirm(
            message = entry.localizedDisplayName(requireContext()),
            positive = if (pinned) R.string.control_unpin else R.string.control_pin,
            negative = android.R.string.cancel,
            destructive = false,
        ) { viewModel.toggleFavorite(entry) }
    }

    internal fun openVpn() = requireNavigator().selectTab(VPN_FEATURE)

    internal fun openHubSection(card: HubCard) {
        val featureId = when (card) {
            is HubCard.Alarm -> "alarms"
            is HubCard.TimerCard -> "timers"
            is HubCard.Reminder -> "reminders"
        }
        requireNavigator().selectTab(featureId)
    }

    internal fun hubAlarmNeedsChoice(card: HubCard.Alarm): Boolean =
        !card.ringing &&
            card.schedule.enabled &&
            alarmDisableChoice(card.schedule) == AlarmDisableChoice.ASK

    internal fun showHubAlarmDisableChoice(card: HubCard.Alarm) {
        MaterialAlertDialogBuilder(requireContext())
            .setTitle(R.string.control_alarm_disable_title)
            .setMessage(R.string.control_alarm_disable_message)
            .setNeutralButton(R.string.control_alarm_disable_skip_once) { _, _ -> viewModel.skipAlarmOccurrence(card) }
            .setPositiveButton(R.string.control_alarm_disable_turn_off) { _, _ -> viewModel.disableAlarm(card) }
            .setNegativeButton(android.R.string.cancel, null)
            .show()
    }

    private fun texts() = ControlTexts(
        otherDevices = getString(R.string.control_section_other),
        scenarios = getString(R.string.control_section_scenarios),
        unavailable = getString(R.string.control_section_unavailable),
        voice = getString(R.string.control_section_voice),
        onSummary = { on, total -> getString(R.string.control_on_summary, on, total) },
        actions = getString(R.string.control_section_actions),
        all = getString(R.string.control_filter_all),
        lights = getString(R.string.control_filter_lights),
        switches = getString(R.string.control_filter_switches),
    )

    internal fun ControlEntry.isOn(): Boolean = power == dev.proflyder.home.core.model.PowerState.ON
}

private const val VPN_FEATURE = "vpn"
