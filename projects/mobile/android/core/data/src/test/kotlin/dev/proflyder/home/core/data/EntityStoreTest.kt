package dev.proflyder.home.core.data

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class EntityStoreTest {
    private data class Item(val id: String, val rank: Int)

    private var remote = listOf(Item("b", 2), Item("a", 1))
    private var gate: CompletableDeferred<Unit>? = null
    private val failure = RemoteFailure.Timeout

    private fun TestScope.store(fails: Boolean = false) = EntityStore(
        scope = TestScope(UnconfinedTestDispatcher(testScheduler)),
        keyOf = Item::id,
        order = compareBy(Item::rank),
    ) {
        gate?.await()
        if (fails) Outcome.Failure(failure) else Outcome.Success(remote)
    }

    @Test
    fun refreshLoadsSortedEntitiesAndSettlesTheLoadState() = runTest {
        val store = store()
        assertTrue(store.load.value.refreshing)
        store.refresh(RefreshMode.SOFT)
        assertEquals(listOf("a", "b"), store.items.value.map { it.id })
        assertEquals(setOf("a", "b"), store.entities.value.keys)
        assertFalse(store.load.value.refreshing)
    }

    @Test
    fun aFailedRefreshKeepsTheEntitiesAndReportsTheFailure() = runTest {
        val store = store(fails = true)
        store.replaceAll(remote)
        store.refresh(RefreshMode.SOFT)
        assertEquals(2, store.items.value.size)
        assertEquals(failure, store.load.value.failure)
    }

    @Test
    fun upsertAndRemoveChangeOneEntityAndKeepOrder() = runTest {
        val store = store()
        store.replaceAll(remote)
        store.upsert(Item("c", 0))
        store.remove("b")
        assertEquals(listOf("c", "a"), store.items.value.map { it.id })
    }

    @Test
    fun anOptimisticChangeIsVisibleAtOnceAndSettlesWithTheServerAnswer() = runTest {
        val store = store()
        store.replaceAll(remote)
        val answer = CompletableDeferred<Outcome<Item>>()
        val job = launch(UnconfinedTestDispatcher(testScheduler)) { store.optimistic({ it - "a" }, { answer.await() }) { upsert(it) } }
        assertNull(store["a"])
        answer.complete(Outcome.Success(Item("a", 5)))
        job.join()
        assertEquals(listOf("b", "a"), store.items.value.map { it.id })
    }

    @Test
    fun aFailedOptimisticChangeRollsBack() = runTest {
        val store = store()
        store.replaceAll(remote)
        val result = store.optimistic({ it - "a" }, { Outcome.Failure(failure) }) { remove("a") }
        assertTrue(result is Outcome.Failure)
        assertEquals(listOf("a", "b"), store.items.value.map { it.id })
    }

    @Test
    fun aBackgroundSyncKeepsLocalWritesMadeWhileItWasInFlight() = runTest {
        val store = store()
        store.replaceAll(remote)
        gate = CompletableDeferred()
        store.refresh(RefreshMode.SOFT)
        store.upsert(Item("a", 9))
        store.remove("b")
        remote = listOf(Item("a", 1), Item("b", 2), Item("d", 3))
        gate?.complete(Unit)
        assertEquals(listOf("d", "a"), store.items.value.map { it.id })
        assertEquals(9, store["a"]?.rank)
    }

    @Test
    fun observeEmitsOneEntity() = runTest {
        val store = store()
        store.replaceAll(remote)
        val seen = mutableListOf<Item?>()
        val job = launch(UnconfinedTestDispatcher(testScheduler)) { store.observe("a").collect { seen += it } }
        store.upsert(Item("b", 7))
        store.remove("a")
        job.cancel()
        assertEquals(listOf(Item("a", 1), null), seen)
    }
}
