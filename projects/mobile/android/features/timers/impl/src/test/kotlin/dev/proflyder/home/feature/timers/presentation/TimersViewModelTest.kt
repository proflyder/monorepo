package dev.proflyder.home.feature.timers.presentation

import dev.proflyder.home.core.testing.FakeSyncSignals
import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.ServerClock
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.model.TimerState
import dev.proflyder.home.core.testing.FakeClock
import dev.proflyder.home.core.testing.ScheduleFixtures.timer
import dev.proflyder.home.feature.timers.TIMER_PRESETS
import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.data.SyncEntity
import dev.proflyder.home.core.data.SyncOp
import dev.proflyder.home.core.data.SyncSignal
import dev.proflyder.home.feature.timers.data.StoreTimerRepository
import dev.proflyder.home.feature.timers.data.TimerRemote
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import dev.proflyder.home.feature.timers.domain.StartTimer
import dev.proflyder.home.feature.timers.toTimerInput
import kotlin.time.Duration.Companion.hours
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class TimersViewModelTest {
    private val dispatcher = StandardTestDispatcher()
    private val clock = FakeClock(wallMillis = 1_000_000L)
    private val signals = FakeSyncSignals()
    private val remote = FakeTimerRemote(listOf(timer("tea", dueUnix = 1_060L)))
    private val repository by lazy {
        StoreTimerRepository(remote, FeatureLifetime(CoroutineScope(Dispatchers.Main + SupervisorJob())), signals)
    }

    @Before
    fun setUp() = Dispatchers.setMain(dispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun viewModel() = TimersViewModel(
        startTimer = StartTimer(repository),
        repository = repository,
        serverClock = ServerClock(clock),
        settings = NoSettings,
        defaultLabel = "Timer",
    )

    @Test
    fun theCountdownTicksOnlyWhileTheScreenIsVisible() = runTest(dispatcher) {
        val viewModel = viewModel()
        viewModel.visibility.started()
        runCurrent()
        assertEquals(60L, viewModel.state.value.rows.single().remainingSeconds)
        clock.advanceMillis(5_000)
        advanceTimeBy(1_100)
        assertEquals(55L, viewModel.state.value.rows.single().remainingSeconds)
        viewModel.visibility.stopped()
        runCurrent()
        clock.advanceMillis(5_000)
        advanceTimeBy(5_000)
        assertEquals(55L, viewModel.state.value.rows.single().remainingSeconds)
    }

    @Test
    fun showingTheScreenAgainMakesNoRequest() = runTest(dispatcher) {
        val viewModel = viewModel()
        viewModel.visibility.started()
        runCurrent()
        val loads = remote.listCalls
        viewModel.visibility.stopped()
        viewModel.visibility.started()
        clock.advanceMillis(61_000)
        advanceTimeBy(61_000)
        viewModel.visibility.stopped()
        runCurrent()
        assertEquals(loads, remote.listCalls)
    }

    @Test
    fun aSyncSignalReloadsTimersOnce() = runTest(dispatcher) {
        val viewModel = viewModel()
        runCurrent()
        val loads = remote.listCalls
        signals.publish(SyncSignal.Changed(SyncEntity.TIMERS, SyncOp.UPSERT, "tea"))
        runCurrent()
        assertEquals(loads + 1, remote.listCalls)
        assertEquals(1, viewModel.state.value.rows.size)
    }

    @Test
    fun presetsFillTheFormAndAnInvalidDurationIsShownInline() = runTest(dispatcher) {
        val viewModel = viewModel()
        viewModel.dispatch(TimersIntent.SetDuration("soon"))
        viewModel.dispatch(TimersIntent.Start)
        assertTrue(viewModel.state.value.form.durationInvalid)
        viewModel.dispatch(TimersIntent.Preset(TIMER_PRESETS[1]))
        assertEquals("5m", viewModel.state.value.form.duration)
        assertFalse(viewModel.state.value.form.durationInvalid)
        assertEquals("1h 30m 5s", (1.hours + 30.minutes + 5.seconds).toTimerInput())
    }

    private object NoSettings : ConnectionSettingsStore {
        override val config: StateFlow<ConnectionConfig> = MutableStateFlow(ConnectionConfig())
        override val tokenStatus: StateFlow<TokenStatus> = MutableStateFlow(TokenStatus.ABSENT)

        override suspend fun updateToken(bearerToken: String?): Outcome<Unit> = Outcome.Success(Unit)
    }
}

internal class FakeTimerRemote(private val timers: List<Timer>) : TimerRemote {
    var listCalls = 0

    override suspend fun list(): Outcome<List<Timer>> = Outcome.Success(timers).also { listCalls++ }

    override suspend fun start(label: String, durationSeconds: Long, idempotencyKey: String): Outcome<Timer> =
        Outcome.Success(timer(label, dueUnix = durationSeconds))

    override suspend fun pause(id: String): Outcome<Timer> = Outcome.Success(timers.first().copy(state = TimerState.PAUSED))

    override suspend fun resume(id: String): Outcome<Timer> = Outcome.Success(timers.first())

    override suspend fun cancel(id: String): Outcome<Timer> = Outcome.Success(timers.first())
}
