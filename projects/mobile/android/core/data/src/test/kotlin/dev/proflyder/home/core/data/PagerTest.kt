package dev.proflyder.home.core.data

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class PagerTest {
    @Test
    fun theFirstPageAsksForTwentyItemsAndKeepsTheCursor() = runTest {
        val source = FakeSource(
            Outcome.Success(Page(items("a", "b"), "next")),
        )
        val pager = pager(source)

        pager.refresh(RefreshMode.HARD)
        advanceUntilIdle()

        assertEquals(listOf("a", "b"), pager.state.value.items.map { it.id })
        assertEquals(listOf<String?>(null), source.cursors)
        assertEquals(listOf(DEFAULT_PAGE_SIZE), source.limits)
        assertEquals(false, pager.state.value.endReached)
    }

    @Test
    fun theNextPageUsesTheCursorAndDeduplicatesKeys() = runTest {
        val source = FakeSource(
            Outcome.Success(Page(items("a", "b"), "next")),
            Outcome.Success(Page(items("b", "c"), null)),
        )
        val pager = pager(source)

        pager.refresh(RefreshMode.HARD)
        advanceUntilIdle()
        pager.loadMore()
        advanceUntilIdle()

        assertEquals(listOf("a", "b", "c"), pager.state.value.items.map { it.id })
        assertEquals(listOf(null, "next"), source.cursors)
        assertTrue(pager.state.value.endReached)
    }

    @Test
    fun aRefreshReplacesTheListWhileLoadMoreAppends() = runTest {
        val source = FakeSource(
            Outcome.Success(Page(items("a"), "one")),
            Outcome.Success(Page(items("b"), "two")),
            Outcome.Success(Page(items("c"), null)),
        )
        val pager = pager(source)

        pager.refresh(RefreshMode.HARD)
        advanceUntilIdle()
        pager.loadMore()
        advanceUntilIdle()
        pager.refresh(RefreshMode.HARD)
        advanceUntilIdle()

        assertEquals(listOf("c"), pager.state.value.items.map { it.id })
        assertFalse(pager.state.value.refreshing)
    }

    @Test
    fun aFailedRefreshIsVisibleAndRetried() = runTest {
        val source = FakeSource(
            Outcome.Failure(RemoteFailure.Offline("offline")),
            Outcome.Success(Page(items("a"), null)),
        )
        val pager = pager(source)

        pager.refresh(RefreshMode.HARD)
        advanceUntilIdle()
        assertEquals(RemoteFailure.Offline("offline"), pager.state.value.failure)
        assertFalse(pager.state.value.hasItems)

        pager.refresh(RefreshMode.HARD)
        advanceUntilIdle()
        assertEquals(listOf("a"), pager.state.value.items.map { it.id })
        assertNull(pager.state.value.failure)
    }

    @Test
    fun aFailedAppendKeepsTheLoadedItems() = runTest {
        val source = FakeSource(
            Outcome.Success(Page(items("a"), "next")),
            Outcome.Failure(RemoteFailure.Timeout),
            Outcome.Success(Page(items("b"), null)),
        )
        val pager = pager(source)

        pager.refresh(RefreshMode.HARD)
        advanceUntilIdle()
        pager.loadMore()
        advanceUntilIdle()
        assertEquals(listOf("a"), pager.state.value.items.map { it.id })
        assertEquals(RemoteFailure.Timeout, pager.state.value.appendFailure)

        pager.loadMore()
        advanceUntilIdle()
        assertEquals(listOf("a", "b"), pager.state.value.items.map { it.id })
        assertNull(pager.state.value.appendFailure)
    }

    @Test
    fun anOptimisticUpdateRollsBackWhenTheRequestFails() = runTest {
        val source = FakeSource(Outcome.Success(Page(items("a"), null)))
        val pager = pager(source)
        pager.refresh(RefreshMode.HARD)
        advanceUntilIdle()

        val result = pager.optimistic(
            { items -> items.map { it.copy(read = true) } },
            { Outcome.Failure(RemoteFailure.Offline("offline")) },
        )

        assertTrue(result is Outcome.Failure)
        assertEquals(false, pager.state.value.items.single().read)
    }

    @Test
    fun anOptimisticUpdateKeepsTheChangeWhenTheRequestSucceeds() = runTest {
        val source = FakeSource(Outcome.Success(Page(items("a"), null)))
        val pager = pager(source)
        pager.refresh(RefreshMode.HARD)
        advanceUntilIdle()

        val result = pager.optimistic(
            { items -> items.map { it.copy(read = true) } },
            { Outcome.Success(Unit) },
        )

        assertTrue(result is Outcome.Success)
        assertEquals(true, pager.state.value.items.single().read)
    }

    @Test
    fun aRemovedEntryReturnsToItsPlace() = runTest {
        val source = FakeSource(Outcome.Success(Page(items("a", "b", "c"), null)))
        val pager = pager(source)
        pager.refresh(RefreshMode.HARD)
        advanceUntilIdle()

        val removal = checkNotNull(pager.removeLocal("b"))
        assertEquals(listOf("a", "c"), pager.state.value.items.map { it.id })
        pager.restoreLocal(removal)

        assertEquals(listOf("a", "b", "c"), pager.state.value.items.map { it.id })
    }

    private fun TestScope.pager(source: PageSource<Row>): Pager<Row> = Pager(this, source, Row::id)

    private class FakeSource(private vararg val responses: Outcome<Page<Row>>) : PageSource<Row> {
        val cursors = mutableListOf<String?>()
        val limits = mutableListOf<Int>()
        private var index = 0

        override suspend fun load(cursor: String?, limit: Int): Outcome<Page<Row>> {
            cursors += cursor
            limits += limit
            val response = responses.getOrElse(index) { responses.last() }
            index += 1
            return response
        }
    }

    private data class Row(val id: String, val read: Boolean = false)

    private companion object {
        fun items(vararg ids: String): List<Row> = ids.map(::Row)
    }
}
