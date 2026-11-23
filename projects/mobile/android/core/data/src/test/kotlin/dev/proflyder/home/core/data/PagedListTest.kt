package dev.proflyder.home.core.data

import dev.proflyder.home.core.model.Keyed
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
class PagedListTest {
    @Test
    fun theFirstPageAndTheNextPageUseTheCursor() = runTest {
        val source = FakeSource(
            Outcome.Success(Page(rows("a", "b"), "next")),
            Outcome.Success(Page(rows("b", "c"), null)),
        )
        val list = list(source)

        list.dispatch(PagingAction.Refresh(RefreshMode.HARD))
        advanceUntilIdle()
        list.dispatch(PagingAction.LoadMore)
        advanceUntilIdle()

        assertEquals(listOf("a", "b", "c"), list.state.value.items.map { it.key })
        assertEquals(listOf(null, "next"), source.requests.map { it.cursor })
        assertEquals(listOf(20, 20), source.requests.map { it.size })
        assertEquals(LoadState.End, list.state.value.append)
    }

    @Test
    fun aFailedRefreshIsRetried() = runTest {
        val source = FakeSource(
            Outcome.Failure(RemoteFailure.Offline("offline")),
            Outcome.Success(Page(rows("a"), null)),
        )
        val list = list(source)

        list.dispatch(PagingAction.Refresh(RefreshMode.HARD))
        advanceUntilIdle()
        assertEquals(LoadState.Failed(RemoteFailure.Offline("offline")), list.state.value.refresh)

        list.dispatch(PagingAction.Retry)
        advanceUntilIdle()
        assertEquals(listOf("a"), list.state.value.items.map { it.key })
        assertEquals(LoadState.End, list.state.value.refresh)
    }

    @Test
    fun aReloadMergesTheFirstPageAndKeepsTheTail() = runTest {
        val first = (1..20).map { Row("n-$it") }
        val second = (21..40).map { Row("n-$it") }
        val source = FakeSource(
            Outcome.Success(Page(first, "20")),
            Outcome.Success(Page(second, null)),
            Outcome.Success(Page(listOf(Row("fresh")) + first.map { it.copy(read = true) }, null)),
        )
        val list = list(source)
        list.dispatch(PagingAction.Refresh(RefreshMode.HARD))
        advanceUntilIdle()
        list.dispatch(PagingAction.LoadMore)
        advanceUntilIdle()

        list.dispatch(PagingAction.Reload)
        advanceUntilIdle()

        val items = list.state.value.items
        assertEquals(41, items.size)
        assertEquals("fresh", items.first().key)
        assertTrue(items.first { it.key == "n-1" }.read)
        assertEquals("n-40", items.last().key)
    }

    @Test
    fun hiddenItemsAreFilteredFromLoadedPages() = runTest {
        val source = FakeSource(
            Outcome.Success(Page(rows("a", "b"), "next")),
            Outcome.Success(Page(rows("b", "c"), null)),
        )
        val list = list(source)
        list.dispatch(PagingAction.Refresh(RefreshMode.HARD))
        advanceUntilIdle()

        assertTrue(list.hide("b"))
        assertEquals(listOf("a"), list.state.value.items.map { it.key })
        list.dispatch(PagingAction.LoadMore)
        advanceUntilIdle()
        assertEquals(listOf("a", "c"), list.state.value.items.map { it.key })
    }

    @Test
    fun aHiddenItemComesBackAtItsPlace() = runTest {
        val source = FakeSource(Outcome.Success(Page(rows("a", "b", "c"), null)))
        val list = list(source)
        list.dispatch(PagingAction.Refresh(RefreshMode.HARD))
        advanceUntilIdle()

        list.hide("b")
        list.reveal("b")

        assertEquals(listOf("a", "b", "c"), list.state.value.items.map { it.key })
    }

    @Test
    fun anOptimisticUpdateRollsBackOnlyItsOwnItem() = runTest {
        val source = FakeSource(Outcome.Success(Page(rows("a", "c"), null)))
        val list = list(source)
        list.dispatch(PagingAction.Refresh(RefreshMode.HARD))
        advanceUntilIdle()
        val gate = CompletableDeferred<Outcome<Unit>>()

        val failing = async {
            list.commit(ListEdit.Update("a") { it.copy(read = true) }, { gate.await() })
        }
        runCurrent()
        val passing = list.commit(ListEdit.Update("c") { it.copy(read = true) }, { Outcome.Success(Unit) })
        gate.complete(Outcome.Failure(RemoteFailure.Offline("offline")))
        val failed = failing.await()

        assertTrue(passing is Outcome.Success)
        assertTrue(failed is Outcome.Failure)
        assertEquals(false, list.state.value.item("a")?.read)
        assertEquals(true, list.state.value.item("c")?.read)
    }

    @Test
    fun aFailedUpdateKeepsAPageLoadedInTheMeantime() = runTest {
        val source = FakeSource(
            Outcome.Success(Page(rows("a"), "next")),
            Outcome.Success(Page(rows("b"), null)),
        )
        val list = list(source)
        list.dispatch(PagingAction.Refresh(RefreshMode.HARD))
        advanceUntilIdle()
        val gate = CompletableDeferred<Outcome<Unit>>()

        val failing = async {
            list.commit(ListEdit.Update("a") { it.copy(read = true) }, { gate.await() })
        }
        runCurrent()
        list.dispatch(PagingAction.LoadMore)
        advanceUntilIdle()
        gate.complete(Outcome.Failure(RemoteFailure.Offline("offline")))
        failing.await()

        assertEquals(listOf("a", "b"), list.state.value.items.map { it.key })
        assertEquals(false, list.state.value.item("a")?.read)
    }

    @Test
    fun aFailedUpdateAllKeepsAConcurrentToggle() = runTest {
        val source = FakeSource(
            Outcome.Success(Page(rows("a", "b"), "next")),
            Outcome.Success(Page(rows("c"), null)),
        )
        val list = list(source)
        list.dispatch(PagingAction.Refresh(RefreshMode.HARD))
        advanceUntilIdle()
        val gate = CompletableDeferred<Outcome<Unit>>()

        val failing = async {
            list.commit(ListEdit.UpdateAll { it.copy(read = true) }, { gate.await() })
        }
        runCurrent()
        list.dispatch(PagingAction.LoadMore)
        advanceUntilIdle()
        list.commit(ListEdit.Update("a") { it.copy(read = false) }, { Outcome.Success(Unit) })
        gate.complete(Outcome.Failure(RemoteFailure.Offline("offline")))
        failing.await()

        assertEquals(false, list.state.value.item("a")?.read)
        assertEquals(false, list.state.value.item("b")?.read)
        assertEquals(false, list.state.value.item("c")?.read)
    }

    @Test
    fun aFailedRemoveRevealsTheItemAgain() = runTest {
        val source = FakeSource(Outcome.Success(Page(rows("a", "b"), null)))
        val list = list(source)
        list.dispatch(PagingAction.Refresh(RefreshMode.HARD))
        advanceUntilIdle()

        val result = list.commit(ListEdit.Remove("a"), { Outcome.Failure(RemoteFailure.Offline("offline")) })

        assertTrue(result is Outcome.Failure)
        assertEquals(listOf("a", "b"), list.state.value.items.map { it.key })
    }

    @Test
    fun aSuccessfulRemoveKeepsTheItemHidden() = runTest {
        val source = FakeSource(
            Outcome.Success(Page(rows("a", "b"), "next")),
            Outcome.Success(Page(rows("a", "c"), null)),
        )
        val list = list(source)
        list.dispatch(PagingAction.Refresh(RefreshMode.HARD))
        advanceUntilIdle()

        val result = list.commit(ListEdit.Remove("a"), { Outcome.Success(Unit) })
        list.dispatch(PagingAction.LoadMore)
        advanceUntilIdle()

        assertTrue(result is Outcome.Success)
        assertEquals(listOf("b", "c"), list.state.value.items.map { it.key })
        assertFalse(list.state.value.items.any { it.key == "a" })
        assertNull(list.state.value.item("missing"))
    }

    private fun TestScope.list(source: PagedSource<Row>): PagedList<Row> = PagedList(this, source)

    private class FakeSource(private vararg val responses: Outcome<Page<Row>>) : PagedSource<Row> {
        val requests = mutableListOf<PageRequest>()
        private var index = 0

        override suspend fun load(request: PageRequest): Outcome<Page<Row>> {
            requests += request
            val response = responses.getOrElse(index) { responses.last() }
            index += 1
            return response
        }
    }

    private data class Row(override val key: String, val read: Boolean = false) : Keyed

    private companion object {
        fun rows(vararg keys: String): List<Row> = keys.map(::Row)
    }
}
