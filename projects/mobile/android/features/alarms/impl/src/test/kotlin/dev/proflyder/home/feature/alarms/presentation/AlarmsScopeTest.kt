package dev.proflyder.home.feature.alarms.presentation

import dev.proflyder.home.core.testing.FakeSyncSignals
import androidx.lifecycle.ViewModelStore
import dev.proflyder.home.core.data.RefCountedScope
import dev.proflyder.home.core.presentation.viewModel
import dev.proflyder.home.core.testing.ScheduleFixtures.schedule
import dev.proflyder.home.feature.alarms.data.StoreAlarmRepository
import dev.proflyder.home.feature.alarms.domain.SnoozeAlarm
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class AlarmsScopeTest {
    private val dispatcher = StandardTestDispatcher()
    private val host = TestScope(dispatcher)
    private val remote = FakeAlarmRemote(listOf(schedule("a1")))
    private val scope = RefCountedScope(host, MutableStateFlow(false), 5.seconds) { StoreAlarmRepository(remote, it, FakeSyncSignals()) }

    @Before
    fun setUp() = Dispatchers.setMain(dispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun twoScreensShareOneRepositoryAndItIsClearedAfterBothClose() {
        val listStore = ViewModelStore()
        val sectionStore = ViewModelStore()
        val list = scope.viewModel { AlarmsViewModel(it) }.also { listStore.put("list", it) }
        val section = scope.viewModel { UpcomingAlarmsViewModel(it, SnoozeAlarm(it), dev.proflyder.home.core.model.ServerClock(dev.proflyder.home.core.testing.FakeClock())) }
            .also { sectionStore.put("section", it) }
        host.runCurrent()
        assertEquals(2, scope.owners)
        assertSame(scope.instance, scope.instance)
        assertEquals(1, remote.listCalls)
        assertEquals(listOf("a1"), list.state.value.alarms.map { it.id })
        assertNotNull(section)
        listStore.clear()
        assertNotNull(scope.instance)
        sectionStore.clear()
        assertEquals(0, scope.owners)
        host.advanceTimeBy(6.seconds)
        host.runCurrent()
        assertNull(scope.instance)
    }
}
