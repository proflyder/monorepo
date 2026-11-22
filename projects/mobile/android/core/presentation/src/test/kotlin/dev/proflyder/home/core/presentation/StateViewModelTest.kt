package dev.proflyder.home.core.presentation

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class StateViewModelTest {
    private sealed interface CounterIntent {
        data object Increment : CounterIntent
        data object Announce : CounterIntent
    }

    private class CounterViewModel : StateViewModel<Int, CounterIntent, String>(0) {
        override fun handle(intent: CounterIntent) = when (intent) {
            CounterIntent.Increment -> updateState { this + 1 }
            CounterIntent.Announce -> emit("count=${state.value}")
        }
    }

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun intentsReduceTheSingleState() {
        val viewModel = CounterViewModel()
        viewModel.dispatch(CounterIntent.Increment)
        viewModel.dispatch(CounterIntent.Increment)
        assertEquals(2, viewModel.state.value)
    }

    @Test
    fun eventsEmittedWithoutCollectorAreDeliveredOnceLater() = runTest {
        val viewModel = CounterViewModel()
        viewModel.dispatch(CounterIntent.Announce)
        viewModel.dispatch(CounterIntent.Increment)
        viewModel.dispatch(CounterIntent.Announce)
        assertEquals(listOf("count=0", "count=1"), viewModel.events.take(2).toList())
    }
}
