package dev.proflyder.home.feature.time

import androidx.lifecycle.SavedStateHandle
import dev.proflyder.home.core.navigation.RouteArgs
import dev.proflyder.home.core.navigation.RoutePath
import dev.proflyder.home.feature.time.presentation.TimeIntent
import dev.proflyder.home.feature.time.presentation.TimeViewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class TimeRoutesTest {
    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun sectionsAreParsedFromTypedRoutes() {
        val entry = TimeFeatureEntry()
        assertEquals(TimeDestination(TimeSection.TIMERS), entry.parse(path("time/timers")))
        assertNull(entry.parse(path("time/ghost")))
        assertNull(entry.parse(path("time")))
        assertEquals(TimeDestination(TimeSection.REMINDERS), entry.hostedDestination("reminders"))
    }

    @Test
    fun theSelectedSectionComesFromTheRouteAndSurvivesInSavedState() {
        val saved = SavedStateHandle(mapOf(RouteArgs.KEY to RouteArgs("time/timers")))
        val viewModel = TimeViewModel(saved)
        assertEquals(TimeSection.TIMERS, viewModel.state.value.section)

        viewModel.dispatch(TimeIntent.Select(TimeSection.REMINDERS))

        assertEquals(TimeSection.REMINDERS, viewModel.state.value.section)
        assertEquals(TimeSection.REMINDERS, TimeViewModel(saved).state.value.section)
    }

    private fun path(raw: String): RoutePath = checkNotNull(RoutePath.of(raw))
}
