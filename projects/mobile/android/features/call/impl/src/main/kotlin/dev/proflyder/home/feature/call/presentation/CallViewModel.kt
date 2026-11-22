package dev.proflyder.home.feature.call.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.runtime.tickerFlow
import dev.proflyder.home.core.ui.ViewModelSource
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.binding
import dev.proflyder.home.feature.call.CallController
import dev.proflyder.home.feature.call.CallPhase
import dev.proflyder.home.feature.call.CallSnapshot
import dev.proflyder.home.feature.call.audio.CallAudioController
import dev.proflyder.home.feature.call.audio.CallAudioRoute
import dev.proflyder.home.feature.call.audio.CallAudioRouteState
import dev.proflyder.home.feature.call.telecom.CallCoordinator
import dev.zacsweers.metro.Inject
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update

@Inject
internal class CallViewModel(
    private val controller: CallController,
    private val coordinator: CallCoordinator,
    private val audio: CallAudioController,
    private val permissions: CallPermissions,
    private val clock: Clock,
) : StateViewModel<CallScreenState, CallIntent, CallEvent>(CallScreenState()) {

    private val flags = MutableStateFlow(CallScreenFlags())

    init {
        launch { controller.state.filter { it.phase != CallPhase.IDLE }.collect { clearAwaitingStart() } }
        launch { combine(controller.state, audio.state, flags, elapsedSeconds(), ::project).collect(::show) }
    }

    override fun handle(intent: CallIntent) {
        when (intent) {
            is CallIntent.Open -> open(intent.startCall)
            is CallIntent.PermissionsResult -> onPermissions(intent.granted)
            CallIntent.ToggleMute -> controller.setMuted(!controller.state.value.muted)
            CallIntent.HangupOrClose -> hangupOrClose()
            CallIntent.Back -> emit(if (controller.state.value.isLive) CallEvent.MoveToBack else CallEvent.Finish)
            CallIntent.ShowRoutes -> setRouteSheet(visible = true)
            CallIntent.HideRoutes -> setRouteSheet(visible = false)
            is CallIntent.SelectRoute -> selectRoute(intent.route)
        }
    }

    private fun project(
        call: CallSnapshot,
        routes: CallAudioRouteState,
        current: CallScreenFlags,
        elapsed: Long,
    ): Pair<CallScreenState, Boolean> = callScreenState(call, routes, current, elapsed) to shouldClose(call, current)

    private fun show(projection: Pair<CallScreenState, Boolean>) {
        updateState { projection.first }
        if (projection.second) emit(CallEvent.Finish)
    }

    private fun clearAwaitingStart() {
        flags.update { it.copy(awaitingStart = false) }
    }

    private fun open(startCall: Boolean) {
        flags.update { it.copy(opened = true, awaitingStart = it.awaitingStart || startCall) }
        if (!startCall) return
        controller.dismissEnded()
        val missing = permissions.missing()
        if (missing.isEmpty()) coordinator.startOutgoingCall() else emit(CallEvent.RequestPermissions(missing))
    }

    private fun onPermissions(granted: Boolean) {
        if (granted || permissions.microphoneGranted()) {
            coordinator.startOutgoingCall()
        } else {
            controller.reportPermissionDenied()
        }
    }

    private fun hangupOrClose() {
        if (controller.state.value.isLive) {
            controller.hangup()
            return
        }
        controller.dismissEnded()
        emit(CallEvent.Finish)
    }

    private fun setRouteSheet(visible: Boolean) {
        if (visible) audio.refresh()
        flags.update { it.copy(routeSheetVisible = visible) }
    }

    private fun selectRoute(route: CallAudioRoute) {
        audio.select(route)
        setRouteSheet(visible = false)
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    private fun elapsedSeconds(): Flow<Long> = controller.state
        .map { it.activeSinceUnix.takeIf { _ -> it.phase == CallPhase.ACTIVE } }
        .distinctUntilChanged()
        .flatMapLatest { since -> since?.let(::tickFrom) ?: flowOf(0L) }

    private fun tickFrom(sinceUnix: Long): Flow<Long> = tickerFlow(1.seconds).map {
        (clock.wallMillis() / MILLIS_PER_SECOND - sinceUnix).coerceAtLeast(0)
    }

    private companion object {
        const val MILLIS_PER_SECOND = 1_000L
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(CallViewModel::class)
@Inject
class CallViewModelSource internal constructor(
    private val viewModel: () -> CallViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}
