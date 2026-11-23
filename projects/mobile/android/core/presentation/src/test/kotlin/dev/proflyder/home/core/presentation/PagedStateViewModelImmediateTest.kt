package dev.proflyder.home.core.presentation

import dev.proflyder.home.core.data.Page
import dev.proflyder.home.core.data.PageRequest
import dev.proflyder.home.core.data.PagedList
import dev.proflyder.home.core.data.PagedSource
import dev.proflyder.home.core.data.PagedState
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.data.PagingAction
import dev.proflyder.home.core.model.Keyed
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class PagedStateViewModelImmediateTest {
    @Before
    fun setUp() {
        Dispatchers.setMain(UnconfinedTestDispatcher())
    }

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun derivedStateStartsAutomaticallyWithImmediateMain() {
        val viewModel = ImmediateViewModel("ok")

        assertEquals("ok:2", viewModel.state.value.derived)
    }

    private class ImmediateViewModel(prefix: String) : PagedStateViewModel<Row, ImmediateState, ImmediateIntent, ImmediateEvent>(
        ImmediateState(),
        list(),
        CoroutineScope(Dispatchers.Main),
        { Outcome.Success(Unit) },
        derive = { pages, pendingRemoval -> ImmediateState(pages, pendingRemoval, "$prefix:${pages.items.size}") },
    ) {
        override fun pagingFailure(failure: RemoteFailure): ImmediateEvent = ImmediateEvent.Failure(failure)

        override fun handle(intent: ImmediateIntent) = Unit
    }

    private data class ImmediateState(
        override val pages: PagedState<Row> = PagedState(),
        override val pendingRemoval: Row? = null,
        val derived: String = "",
    ) : PagedScreenState<Row>

    private sealed interface ImmediateIntent

    private sealed interface ImmediateEvent {
        data class Failure(val failure: RemoteFailure) : ImmediateEvent
    }

    private data class Row(override val key: String) : Keyed

    private companion object {
        fun list(): PagedList<Row> {
            val list = PagedList(
                CoroutineScope(UnconfinedTestDispatcher()),
                object : PagedSource<Row> {
                    override suspend fun load(request: PageRequest) = Outcome.Success(Page(listOf(Row("a"), Row("b")), null))
                },
            )
            list.dispatch(PagingAction.Refresh(RefreshMode.HARD))
            return list
        }
    }
}
