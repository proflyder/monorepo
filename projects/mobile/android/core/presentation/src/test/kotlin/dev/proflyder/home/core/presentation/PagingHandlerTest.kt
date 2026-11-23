package dev.proflyder.home.core.presentation

import dev.proflyder.home.core.data.Page
import dev.proflyder.home.core.data.PageRequest
import dev.proflyder.home.core.data.PagedList
import dev.proflyder.home.core.data.PagedSource
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.model.Keyed
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class PagingHandlerTest {
    @Test
    fun pagingIntentsAreRoutedToTheList() = runTest {
        val handler = handler()

        handler.dispatch(PagingIntent.Refresh(RefreshMode.HARD))
        advanceUntilIdle()

        assertEquals(listOf("a", "b"), handler.state.value.items.map { it.key })
    }

    @Test
    fun retryAfterAFailureLoadsAgain() = runTest {
        val source = FakeSource(
            Outcome.Failure(RemoteFailure.Offline("offline")),
            Outcome.Success(Page(listOf(Row("a")), null)),
        )
        val handler = handler(source)

        handler.dispatch(PagingIntent.Refresh(RefreshMode.HARD))
        advanceUntilIdle()
        handler.dispatch(PagingIntent.Retry)
        advanceUntilIdle()

        assertEquals(listOf("a"), handler.state.value.items.map { it.key })
    }

    @Test
    fun listItemRemoveAndUndoShareTheSameUndoWindow() = runTest {
        val handler = handler()
        handler.dispatch(PagingIntent.Refresh(RefreshMode.HARD))
        advanceUntilIdle()

        handler.dispatch(ListItemIntent.Remove("a"))

        assertEquals(listOf("b"), handler.state.value.items.map { it.key })
        assertEquals("a", handler.pendingUndo.value?.key)

        handler.dispatch(ListItemIntent.Undo)

        assertEquals(listOf("a", "b"), handler.state.value.items.map { it.key })
        assertNull(handler.pendingUndo.value)
    }

    private fun TestScope.handler(source: PagedSource<Row> = FakeSource(Outcome.Success(Page(listOf(Row("a"), Row("b")), null)))): PagingHandler<Row> {
        val list = PagedList(this, source)
        val undo = UndoableRemoval(list, this, this, { }, duration = kotlin.time.Duration.parse("100ms"))
        return PagingHandler(list, undo)
    }

    private class FakeSource(private vararg val responses: Outcome<Page<Row>>) : PagedSource<Row> {
        private var index = 0

        override suspend fun load(request: PageRequest): Outcome<Page<Row>> =
            responses.getOrElse(index++) { responses.last() }
    }

    private data class Row(override val key: String) : Keyed
}
