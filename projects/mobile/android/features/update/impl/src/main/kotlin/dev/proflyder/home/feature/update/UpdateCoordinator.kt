package dev.proflyder.home.feature.update

import dev.proflyder.home.core.model.AppLogger
import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.model.warn
import dev.proflyder.home.core.runtime.AppActivation
import dev.proflyder.home.core.runtime.AppForeground
import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.core.runtime.AppVersion
import dev.proflyder.home.core.runtime.MainDispatcher
import dev.proflyder.home.feature.update.data.AppUpdater
import dev.proflyder.home.feature.update.data.UpdateCheckStore
import dev.proflyder.home.feature.update.data.UpdateEvent
import dev.proflyder.home.feature.update.data.UpdateResult
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch

@SingleIn(AppScope::class)
@Inject
internal class UpdateCoordinator(
    private val updater: AppUpdater,
    private val checks: UpdateCheckStore,
    private val clock: Clock,
    private val activation: AppActivation,
    private val foreground: AppForeground,
    private val version: AppVersion,
    private val logger: AppLogger,
    private val scope: CoroutineScope,
    @MainDispatcher private val main: CoroutineDispatcher,
) : UpdateCenter, AppStartable {
    private val mutableState = MutableStateFlow(UpdateState(currentVersion = version.name))

    override val supported: Boolean get() = updater.supported

    override val state: StateFlow<UpdateState> = mutableState.asStateFlow()

    override fun start() {
        foreground.state
            .filter { it.foreground }
            .onEach { onForeground() }
            .launchIn(scope)
    }

    override fun checkNow() {
        if (!supported) return
        scope.launch(main) { check(manual = true) }
    }

    override fun signIn() {
        if (!supported) return
        scope.launch(main) { runSignIn() }
    }

    override fun install() {
        if (!supported) return
        scope.launch(main) { runUpdate() }
    }

    override fun dismissPrompt() {
        mutableState.value = mutableState.value.copy(promptVisible = false)
    }

    private fun onForeground() {
        if (!supported) return
        scope.launch(main) { runForegroundCheck() }
    }

    private suspend fun runForegroundCheck() {
        activation.awaitActive()
        val elapsed = clock.wallMillis() - checks.lastCheckMillis()
        if (elapsed < CHECK_INTERVAL_MILLIS) return
        check(manual = false)
    }

    private suspend fun check(manual: Boolean) {
        if (busy()) return
        mutableState.value = idleState(UpdatePhase.CHECKING).copy(promptVisible = manual)
        val result = updater.check()
        checks.recordCheck(clock.wallMillis())
        applyCheckResult(result, manual)
    }

    private suspend fun runSignIn() {
        if (busy()) return
        mutableState.value = mutableState.value.copy(phase = UpdatePhase.SIGNING_IN, failure = null)
        when (val result = updater.signIn()) {
            is UpdateResult.Success -> retryAfterSignIn()
            is UpdateResult.Failure -> applySignInFailure(result.kind)
        }
    }

    private suspend fun retryAfterSignIn() {
        mutableState.value = mutableState.value.copy(phase = UpdatePhase.IDLE)
        check(manual = true)
    }

    private suspend fun runUpdate() {
        if (busy()) return
        mutableState.value = mutableState.value.copy(
            phase = UpdatePhase.DOWNLOADING,
            progressPercent = 0,
            failure = null,
            promptVisible = true,
        )
        updater.update().collect(::applyUpdateEvent)
    }

    private fun applyCheckResult(result: UpdateResult<AvailableUpdate>, manual: Boolean): Unit = when (result) {
        is UpdateResult.Success -> showAvailable(result.value)
        is UpdateResult.Failure -> applyCheckFailure(result.kind, manual)
    }

    private fun showAvailable(update: AvailableUpdate) {
        mutableState.value = idleState(UpdatePhase.AVAILABLE).copy(available = update, promptVisible = true)
    }

    private fun applyCheckFailure(kind: UpdateFailureKind, manual: Boolean) {
        if (!manual && isQuiet(kind)) {
            logger.warn(TAG, "automatic update check failed: $kind")
            mutableState.value = idleState(UpdatePhase.IDLE)
            return
        }
        val phase = when (kind) {
            UpdateFailureKind.NOT_AVAILABLE -> UpdatePhase.UP_TO_DATE
            UpdateFailureKind.SIGN_IN_REQUIRED -> UpdatePhase.SIGN_IN_REQUIRED
            else -> UpdatePhase.FAILED
        }
        val prompt = manual || kind == UpdateFailureKind.SIGN_IN_REQUIRED
        mutableState.value = idleState(phase).copy(failure = kind, promptVisible = prompt)
    }

    private fun applySignInFailure(kind: UpdateFailureKind) {
        val phase = when (kind) {
            UpdateFailureKind.SIGN_IN_CANCELLED -> UpdatePhase.SIGN_IN_REQUIRED
            else -> UpdatePhase.FAILED
        }
        mutableState.value = mutableState.value.copy(phase = phase, failure = kind, promptVisible = true)
    }

    private fun applyUpdateEvent(event: UpdateEvent) {
        mutableState.value = when (event) {
            is UpdateEvent.Downloading -> mutableState.value.copy(progressPercent = event.percent)
            UpdateEvent.Downloaded -> mutableState.value.copy(phase = UpdatePhase.INSTALLING, progressPercent = null)
            UpdateEvent.Completed -> mutableState.value.copy(phase = UpdatePhase.INSTALLING, progressPercent = null)
            UpdateEvent.Cancelled -> mutableState.value.copy(phase = UpdatePhase.AVAILABLE, progressPercent = null)
            is UpdateEvent.Failed -> mutableState.value.copy(
                phase = UpdatePhase.FAILED,
                progressPercent = null,
                failure = event.kind,
                promptVisible = true,
            )
        }
    }

    private fun idleState(phase: UpdatePhase): UpdateState = UpdateState(currentVersion = version.name, phase = phase)

    private fun busy(): Boolean = mutableState.value.phase in BUSY_PHASES

    private fun isQuiet(kind: UpdateFailureKind): Boolean =
        kind != UpdateFailureKind.SIGN_IN_REQUIRED && kind != UpdateFailureKind.NOT_AVAILABLE

    private companion object {
        const val TAG = "AppUpdates"
        const val CHECK_INTERVAL_MILLIS = 6L * 60L * 60L * 1000L

        val BUSY_PHASES = setOf(
            UpdatePhase.CHECKING,
            UpdatePhase.SIGNING_IN,
            UpdatePhase.DOWNLOADING,
            UpdatePhase.INSTALLING,
        )
    }
}
