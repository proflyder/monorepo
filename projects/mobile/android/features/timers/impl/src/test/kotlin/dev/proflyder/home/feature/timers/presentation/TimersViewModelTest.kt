package dev.proflyder.home.feature.timers.presentation

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.ServerClock
import dev.proflyder.home.core.model.ServerClockSync
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.model.TimerState
import dev.proflyder.home.core.testing.FakeClock
import dev.proflyder.home.core.testing.ScheduleFixtures.timer
import dev.proflyder.home.feature.timers.TIMER_PRESETS
import dev.proflyder.home.feature.timers.TimerRepository
import dev.proflyder.home.feature.timers.domain.LoadTimers
import dev.proflyder.home.feature.timers.domain.StartTimer
import dev.proflyder.home.feature.timers.toTimerInput
import kotlin.time.Duration.Companion.hours
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
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
    private val repository = FakeTimerRepository(listOf(timer("tea", dueUnix = 1_060L)))

    @Before
    fun setUp() = Dispatchers.setMain(dispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun viewModel() = TimersViewModel(
        loadTimers = LoadTimers(repository, NoSync),
        startTimer = StartTimer(repository),
        repository = repository,
        serverClock = ServerClock(clock),
        settings = NoSettings,
        defaultLabel = "Timer",
        clock = clock,
    )

    @Test
    fun theCountdownTicksOnlyWhileTheScreenIsVisible() = runTest(dispatcher) {
        val viewModel = viewModel()
        val screen = backgroundScope.launch { viewModel.state.collect {} }
        runCurrent()
        assertEquals(60L, viewModel.state.value.rows.single().remainingSeconds)
        clock.advanceMillis(5_000)
        advanceTimeBy(1_100)
        assertEquals(55L, viewModel.state.value.rows.single().remainingSeconds)
        screen.cancel()
        runCurrent()
        clock.advanceMillis(5_000)
        advanceTimeBy(5_000)
        assertEquals(55L, viewModel.state.value.rows.single().remainingSeconds)
    }

    @Test
    fun anExpiredTimerTriggersARefresh() = runTest(dispatcher) {
        val viewModel = viewModel()
        backgroundScope.launch { viewModel.state.collect {} }
        runCurrent()
        val loads = repository.listCalls
        clock.advanceMillis(61_000)
        advanceTimeBy(1_100)
        assertTrue(repository.listCalls > loads)
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

    private object NoSync : ServerClockSync {
        override suspend fun sync(): Outcome<Unit> = Outcome.Success(Unit)
    }

    private object NoSettings : ConnectionSettingsStore {
        override val config: StateFlow<ConnectionConfig> = MutableStateFlow(ConnectionConfig())
        override val tokenStatus: StateFlow<TokenStatus> = MutableStateFlow(TokenStatus.ABSENT)

        override suspend fun updateToken(bearerToken: String?): Outcome<Unit> = Outcome.Success(Unit)
    }
}

internal class FakeTimerRepository(private val timers: List<Timer>) : TimerRepository {
    var listCalls = 0

    override suspend fun list(): Outcome<List<Timer>> = Outcome.Success(timers).also { listCalls++ }

    override suspend fun start(label: String, durationSeconds: Long, idempotencyKey: String): Outcome<Timer> =
        Outcome.Success(timer(label, dueUnix = durationSeconds))

    override suspend fun pause(id: String): Outcome<Timer> = Outcome.Success(timers.first().copy(state = TimerState.PAUSED))

    override suspend fun resume(id: String): Outcome<Timer> = Outcome.Success(timers.first())

    override suspend fun cancel(id: String): Outcome<Timer> = Outcome.Success(timers.first())
}
