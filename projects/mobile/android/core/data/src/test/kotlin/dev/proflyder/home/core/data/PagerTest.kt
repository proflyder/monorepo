package dev.proflyder.home.core.data

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
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
        val source = FakeSource(Outcome.Success(Page(items("a", "b"), "next")))
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
    fun anOptimisticUpdateRollsBackOnlyItsOwnItem() = runTest {
        val source = FakeSource(Outcome.Success(Page(items("a", "c"), null)))
        val pager = pager(source)
        pager.refresh(RefreshMode.HARD)
        advanceUntilIdle()
        val gate = CompletableDeferred<Outcome<Unit>>()

        val failing = async {
            pager.optimisticUpdate("a", { it.copy(read = true) }, { gate.await() })
        }
        runCurrent()
        val passing = pager.optimisticUpdate("c", { it.copy(read = true) }, { Outcome.Success(Unit) })
        gate.complete(Outcome.Failure(RemoteFailure.Offline("offline")))
        val failed = failing.await()

        assertTrue(passing is Outcome.Success)
        assertTrue(failed is Outcome.Failure)
        assertEquals(false, pager.state.value.items.first { it.id == "a" }.read)
        assertEquals(true, pager.state.value.items.first { it.id == "c" }.read)
    }

    @Test
    fun anOptimisticFailureDoesNotDropAPageLoadedInTheMeantime() = runTest {
        val source = FakeSource(
            Outcome.Success(Page(items("a"), "next")),
            Outcome.Success(Page(items("b"), null)),
        )
        val pager = pager(source)
        pager.refresh(RefreshMode.HARD)
        advanceUntilIdle()
        val gate = CompletableDeferred<Outcome<Unit>>()

        val failing = async {
            pager.optimisticUpdate("a", { it.copy(read = true) }, { gate.await() })
        }
        runCurrent()
        pager.loadMore()
        advanceUntilIdle()
        gate.complete(Outcome.Failure(RemoteFailure.Offline("offline")))
        failing.await()

        assertEquals(listOf("a", "b"), pager.state.value.items.map { it.id })
        assertEquals(false, pager.state.value.items.first { it.id == "a" }.read)
    }

    @Test
    fun anOptimisticRemoveRollsBackOnlyTheRemovedItem() = runTest {
        val source = FakeSource(Outcome.Success(Page(items("a", "b", "c"), null)))
        val pager = pager(source)
        pager.refresh(RefreshMode.HARD)
        advanceUntilIdle()

        pager.optimisticUpdate("a", { it.copy(read = true) }, { Outcome.Success(Unit) })
        val result = pager.optimisticRemove("b", { Outcome.Failure(RemoteFailure.Offline("offline")) })

        assertTrue(result is Outcome.Failure)
        assertEquals(listOf("a", "b", "c"), pager.state.value.items.map { it.id })
        assertEquals(true, pager.state.value.items.first { it.id == "a" }.read)
    }

    @Test
    fun aReadAllRollsBackEveryKnownItemButNotANewerPage() = runTest {
        val source = FakeSource(
            Outcome.Success(Page(items("a", "b"), "next")),
            Outcome.Success(Page(items("c"), null)),
        )
        val pager = pager(source)
        pager.refresh(RefreshMode.HARD)
        advanceUntilIdle()
        val gate = CompletableDeferred<Outcome<Unit>>()

        val failing = async {
            pager.optimisticEach({ it.copy(read = true) }, { gate.await() })
        }
        runCurrent()
        pager.loadMore()
        advanceUntilIdle()
        pager.optimisticUpdate("a", { it.copy(read = false) }, { Outcome.Success(Unit) })
        gate.complete(Outcome.Failure(RemoteFailure.Offline("offline")))
        failing.await()

        assertEquals(false, pager.state.value.items.first { it.id == "a" }.read)
        assertEquals(false, pager.state.value.items.first { it.id == "b" }.read)
        assertEquals(false, pager.state.value.items.first { it.id == "c" }.read)
    }

    @Test
    fun reloadMergesTheFirstPageIntoTheLoadedTail() = runTest {
        val first = (1..20).map { Row("n-$it") }
        val second = (21..40).map { Row("n-$it") }
        val source = FakeSource(
            Outcome.Success(Page(first, "20")),
            Outcome.Success(Page(second, null)),
            Outcome.Success(Page(listOf(Row("fresh")) + first.map { it.copy(read = true) }, null)),
        )
        val pager = pager(source)
        pager.refresh(RefreshMode.HARD)
        advanceUntilIdle()
        pager.loadMore()
        advanceUntilIdle()
        assertEquals(40, pager.state.value.items.size)

        pager.reload()
        advanceUntilIdle()

        val items = pager.state.value.items
        assertEquals(41, items.size)
        assertEquals("fresh", items.first().id)
        assertTrue(items.first { it.id == "n-1" }.read)
        assertEquals("n-40", items.last().id)
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
