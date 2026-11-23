package dev.proflyder.home.core.presentation

import dev.proflyder.home.core.data.Page
import dev.proflyder.home.core.data.PageRequest
import dev.proflyder.home.core.data.PagedList
import dev.proflyder.home.core.data.PagedSource
import dev.proflyder.home.core.data.PagedState
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.model.Keyed
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.launch
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class PagedStateViewModelTest {
    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun pagingIntentsGoThroughTheViewModelIntoTheState() = runTest {
        Dispatchers.setMain(UnconfinedTestDispatcher(testScheduler))
        val viewModel = viewModel()

        viewModel.dispatch(PagingIntent.Refresh(RefreshMode.HARD))
        advanceUntilIdle()

        assertEquals(listOf("a", "b"), viewModel.state.value.pages.items.map { it.key })
    }

    @Test
    fun screenIntentsStillReachTheScreenHandler() = runTest {
        Dispatchers.setMain(UnconfinedTestDispatcher(testScheduler))
        val viewModel = viewModel()

        viewModel.dispatch(TestIntent.Open)

        assertEquals(listOf(TestIntent.Open), viewModel.handled)
    }

    @Test
    fun listItemRemoveAndUndoAreStoredInTheScreenState() = runTest {
        Dispatchers.setMain(UnconfinedTestDispatcher(testScheduler))
        val viewModel = viewModel()
        viewModel.dispatch(PagingIntent.Refresh(RefreshMode.HARD))
        advanceUntilIdle()

        viewModel.dispatch(ListItemIntent.Remove("a"))
        assertEquals("a", viewModel.state.value.pendingRemoval?.key)
        assertEquals(listOf("b"), viewModel.state.value.pages.items.map { it.key })

        viewModel.dispatch(ListItemIntent.Undo)
        assertEquals(listOf("a", "b"), viewModel.state.value.pages.items.map { it.key })
        assertNull(viewModel.state.value.pendingRemoval)
    }

    @Test
    fun aFailedCommitRevealsTheItemAndEmitsTheMappedEvent() = runTest {
        Dispatchers.setMain(UnconfinedTestDispatcher(testScheduler))
        val failure = RemoteFailure.Offline("offline")
        val viewModel = viewModel(commit = { Outcome.Failure(failure) })
        val events = mutableListOf<TestEvent>()
        backgroundScope.launch(UnconfinedTestDispatcher(testScheduler)) { viewModel.events.collect { events += it } }
        viewModel.dispatch(PagingIntent.Refresh(RefreshMode.HARD))
        advanceUntilIdle()

        viewModel.dispatch(ListItemIntent.Remove("a"))
        advanceTimeBy(6_001)
        runCurrent()

        assertEquals(listOf("a", "b"), viewModel.state.value.pages.items.map { it.key })
        assertNull(viewModel.state.value.pendingRemoval)
        assertEquals(listOf(TestEvent.Failure(failure)), events)
    }

    private fun TestScope.viewModel(commit: suspend (Row) -> Outcome<Unit> = { Outcome.Success(Unit) }): TestViewModel {
        val list = PagedList(this, object : PagedSource<Row> {
            override suspend fun load(request: PageRequest) = Outcome.Success(Page(listOf(Row("a"), Row("b")), null))
        })
        return TestViewModel(list, CoroutineScope(UnconfinedTestDispatcher(testScheduler)), commit)
    }

    private class TestViewModel(
        list: PagedList<Row>,
        appScope: CoroutineScope,
        commit: suspend (Row) -> Outcome<Unit>,
    ) : PagedStateViewModel<Row, TestState, TestIntent, TestEvent>(
        TestState(),
        list,
        appScope,
        commit,
        derive = { pages, pendingRemoval -> TestState(pages, pendingRemoval) },
    ) {
        val handled = mutableListOf<TestIntent>()

        override fun pagingFailure(failure: RemoteFailure): TestEvent = TestEvent.Failure(failure)

        override fun handle(intent: TestIntent) {
            handled += intent
        }
    }

    private data class TestState(
        override val pages: PagedState<Row> = PagedState(),
        override val pendingRemoval: Row? = null,
    ) : PagedScreenState<Row>

    private sealed interface TestIntent {
        data object Open : TestIntent
    }

    private sealed interface TestEvent {
        data class Failure(val failure: RemoteFailure) : TestEvent
    }

    private data class Row(override val key: String) : Keyed
}
