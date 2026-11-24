package dev.proflyder.home.feature.update

import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.runtime.AppActivation
import dev.proflyder.home.core.runtime.AppForeground
import dev.proflyder.home.core.runtime.AppVersion
import dev.proflyder.home.core.runtime.ForegroundState
import dev.proflyder.home.core.testing.FakeAppLogger
import dev.proflyder.home.core.testing.FakeClock
import dev.proflyder.home.feature.update.data.AppUpdater
import dev.proflyder.home.feature.update.data.UpdateCheckStore
import dev.proflyder.home.feature.update.data.UpdateEvent
import dev.proflyder.home.feature.update.data.UpdateResult
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class UpdateCoordinatorTest {
    private val logger = FakeAppLogger()

    private object Version : AppVersion {
        override val name: String = "0.1.0-test"
        override val debuggable: Boolean = false
    }

    private class FakeUpdater(
        override val supported: Boolean = true,
        var checkResult: UpdateResult<AvailableUpdate> = UpdateResult.Failure(UpdateFailureKind.NOT_AVAILABLE),
        var signInResult: UpdateResult<Unit> = UpdateResult.Success(Unit),
    ) : AppUpdater {
        var checks = 0
        var signIns = 0
        var updates = 0
        val events = mutableListOf<UpdateEvent>()

        override suspend fun signIn(): UpdateResult<Unit> {
            signIns += 1
            return signInResult
        }

        override suspend fun check(): UpdateResult<AvailableUpdate> {
            checks += 1
            return checkResult
        }

        override fun update(): Flow<UpdateEvent> {
            updates += 1
            return flow { events.forEach { event -> emit(event) } }
        }
    }

    private class FakeStore(var last: Long = 0L) : UpdateCheckStore {
        override suspend fun lastCheckMillis(): Long = last

        override suspend fun recordCheck(millis: Long) {
            last = millis
        }
    }

    private class FakeForeground : AppForeground {
        val mutable = MutableStateFlow(ForegroundState(foreground = false, sinceElapsedNanos = 0L))
        override val state: StateFlow<ForegroundState> = mutable

        fun moved(foreground: Boolean) {
            mutable.value = ForegroundState(foreground, 0L)
        }
    }

    private class FakeActivation(active: Boolean = true) : AppActivation {
        val mutable = MutableStateFlow(active)
        override val active: StateFlow<Boolean> = mutable

        override fun activate() {
            mutable.value = true
        }

        override suspend fun awaitActive() {
            active.first { it }
        }
    }

    private fun TestScope.coordinator(
        updater: FakeUpdater = FakeUpdater(),
        store: FakeStore = FakeStore(),
        clock: Clock = FakeClock(NOW),
        foreground: FakeForeground = FakeForeground(),
        activation: FakeActivation = FakeActivation(),
    ): UpdateCoordinator = UpdateCoordinator(
        updater = updater,
        checks = store,
        clock = clock,
        activation = activation,
        foreground = foreground,
        version = Version,
        logger = logger,
        scope = backgroundScope,
        main = UnconfinedTestDispatcher(testScheduler),
    )

    @Test
    fun foregroundChecksWaitForActivationAndRunAtMostOncePerSixHours() = runTest {
        val updater = FakeUpdater()
        val clock = FakeClock(NOW)
        val activation = FakeActivation(active = false)
        val foreground = FakeForeground()
        val coordinator = coordinator(updater, clock = clock, foreground = foreground, activation = activation)
        coordinator.start()

        foreground.moved(true)
        runCurrent()
        assertEquals(0, updater.checks)

        activation.activate()
        runCurrent()
        assertEquals(1, updater.checks)
        assertEquals("0.1.0-test", coordinator.state.value.currentVersion)

        foreground.moved(false)
        runCurrent()
        foreground.moved(true)
        runCurrent()
        assertEquals(1, updater.checks)

        clock.advanceMillis(SIX_HOURS_MILLIS)
        foreground.moved(false)
        runCurrent()
        foreground.moved(true)
        runCurrent()
        assertEquals(2, updater.checks)
    }

    @Test
    fun aManualCheckIgnoresTheSixHourThrottle() = runTest {
        val updater = FakeUpdater()
        val store = FakeStore(last = NOW)
        val coordinator = coordinator(updater, store = store, clock = FakeClock(NOW))

        coordinator.checkNow()
        runCurrent()

        assertEquals(1, updater.checks)
        assertEquals(NOW, store.last)
    }

    @Test
    fun anAvailableReleaseCarriesItsVersionNotesAndPrompt() = runTest {
        val updater = FakeUpdater()
        updater.checkResult = UpdateResult.Success(AvailableUpdate("0.2.0", 2L, "Merge pull request #42"))
        val coordinator = coordinator(updater)

        coordinator.checkNow()
        runCurrent()

        val state = coordinator.state.value
        assertEquals(UpdatePhase.AVAILABLE, state.phase)
        assertEquals("0.2.0", state.available?.versionName)
        assertEquals("Merge pull request #42", state.available?.releaseNotes)
        assertTrue(state.promptVisible)
    }

    @Test
    fun anExpiredTesterSignInIsExplainedAndRetriedAfterSignIn() = runTest {
        val updater = FakeUpdater()
        updater.checkResult = UpdateResult.Failure(UpdateFailureKind.SIGN_IN_REQUIRED)
        val coordinator = coordinator(updater)

        coordinator.checkNow()
        runCurrent()
        assertEquals(UpdatePhase.SIGN_IN_REQUIRED, coordinator.state.value.phase)
        assertEquals(UpdateFailureKind.SIGN_IN_REQUIRED, coordinator.state.value.failure)
        assertTrue(coordinator.state.value.promptVisible)

        updater.checkResult = UpdateResult.Success(AvailableUpdate("0.2.0", 2L, "notes"))
        coordinator.signIn()
        runCurrent()

        assertEquals(1, updater.signIns)
        assertEquals(UpdatePhase.AVAILABLE, coordinator.state.value.phase)
        assertEquals(2, updater.checks)
    }

    @Test
    fun aCancelledSignInKeepsTheSignInPrompt() = runTest {
        val updater = FakeUpdater()
        updater.checkResult = UpdateResult.Failure(UpdateFailureKind.SIGN_IN_REQUIRED)
        updater.signInResult = UpdateResult.Failure(UpdateFailureKind.SIGN_IN_CANCELLED)
        val coordinator = coordinator(updater)

        coordinator.checkNow()
        runCurrent()
        coordinator.signIn()
        runCurrent()

        assertEquals(UpdatePhase.SIGN_IN_REQUIRED, coordinator.state.value.phase)
        assertEquals(UpdateFailureKind.SIGN_IN_CANCELLED, coordinator.state.value.failure)
    }

    @Test
    fun anAutomaticNetworkFailureStaysSilent() = runTest {
        val updater = FakeUpdater()
        updater.checkResult = UpdateResult.Failure(UpdateFailureKind.OFFLINE)
        val foreground = FakeForeground()
        val coordinator = coordinator(updater, foreground = foreground)
        coordinator.start()

        foreground.moved(true)
        runCurrent()

        assertEquals(UpdatePhase.IDLE, coordinator.state.value.phase)
        assertFalse(coordinator.state.value.promptVisible)
        assertTrue(logger.entries.any { it.message.contains("automatic update check failed") })
    }

    @Test
    fun aManualFailureOpensTheErrorPrompt() = runTest {
        val updater = FakeUpdater()
        updater.checkResult = UpdateResult.Failure(UpdateFailureKind.OFFLINE)
        val coordinator = coordinator(updater)

        coordinator.checkNow()
        runCurrent()

        assertEquals(UpdatePhase.FAILED, coordinator.state.value.phase)
        assertEquals(UpdateFailureKind.OFFLINE, coordinator.state.value.failure)
        assertTrue(coordinator.state.value.promptVisible)
    }

    @Test
    fun downloadProgressAndInstallationReachTheState() = runTest {
        val updater = FakeUpdater()
        updater.checkResult = UpdateResult.Success(AvailableUpdate("0.2.0", 2L, "notes"))
        updater.events += listOf(UpdateEvent.Downloading(40), UpdateEvent.Downloaded)
        val coordinator = coordinator(updater)

        coordinator.checkNow()
        runCurrent()
        coordinator.install()
        runCurrent()

        assertEquals(1, updater.updates)
        assertEquals(UpdatePhase.INSTALLING, coordinator.state.value.phase)
        assertNull(coordinator.state.value.progressPercent)
    }

    @Test
    fun aDownloadingStateShowsItsPercent() = runTest {
        val updater = FakeUpdater()
        updater.checkResult = UpdateResult.Success(AvailableUpdate("0.2.0", 2L, "notes"))
        updater.events += UpdateEvent.Downloading(40)
        val coordinator = coordinator(updater)

        coordinator.checkNow()
        runCurrent()
        coordinator.install()
        runCurrent()

        assertEquals(UpdatePhase.DOWNLOADING, coordinator.state.value.phase)
        assertEquals(40, coordinator.state.value.progressPercent)
    }

    @Test
    fun aFailedDownloadOffersAnotherAttempt() = runTest {
        val updater = FakeUpdater()
        updater.checkResult = UpdateResult.Success(AvailableUpdate("0.2.0", 2L, "notes"))
        updater.events += UpdateEvent.Failed(UpdateFailureKind.DOWNLOAD)
        val coordinator = coordinator(updater)

        coordinator.checkNow()
        runCurrent()
        coordinator.install()
        runCurrent()

        assertEquals(UpdatePhase.FAILED, coordinator.state.value.phase)
        assertEquals(UpdateFailureKind.DOWNLOAD, coordinator.state.value.failure)
        assertTrue(coordinator.state.value.promptVisible)
    }

    @Test
    fun dismissingThePromptKeepsTheAvailableRelease() = runTest {
        val updater = FakeUpdater()
        updater.checkResult = UpdateResult.Success(AvailableUpdate("0.2.0", 2L, "notes"))
        val coordinator = coordinator(updater)

        coordinator.checkNow()
        runCurrent()
        coordinator.dismissPrompt()

        assertFalse(coordinator.state.value.promptVisible)
        assertEquals(UpdatePhase.AVAILABLE, coordinator.state.value.phase)
        assertEquals("0.2.0", coordinator.state.value.available?.versionName)
    }

    @Test
    fun anUnsupportedBuildNeverCallsTheSdk() = runTest {
        val updater = FakeUpdater(supported = false)
        val foreground = FakeForeground()
        val coordinator = coordinator(updater, foreground = foreground)
        coordinator.start()

        foreground.moved(true)
        runCurrent()
        coordinator.checkNow()
        coordinator.signIn()
        coordinator.install()
        runCurrent()

        assertEquals(0, updater.checks)
        assertEquals(0, updater.signIns)
        assertEquals(0, updater.updates)
        assertFalse(coordinator.supported)
    }

    private companion object {
        const val NOW = 1_760_000_000_000L
        const val SIX_HOURS_MILLIS = 6L * 60L * 60L * 1000L
    }
}
