package dev.proflyder.home.core.data

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlin.time.Duration.Companion.minutes
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class EntitySyncTest {
    private data class Item(val id: String, val rank: Int)

    private val bus = object : SyncSignals {
        val flow = MutableSharedFlow<SyncSignal>(extraBufferCapacity = 8)
        override val signals = flow
    }
    private var remote = listOf(Item("a", 1), Item("b", 2))
    private var lists = 0
    private val fetched = mutableListOf<String>()
    private var fetchFails = false

    private fun TestScope.followed(fetchOne: Boolean = true): EntityStore<String, Item> {
        val scope = TestScope(UnconfinedTestDispatcher(testScheduler))
        val store = EntityStore(scope, Item::id, compareBy(Item::rank)) {
            lists += 1
            Outcome.Success(remote)
        }
        store.refresh(RefreshMode.SOFT)
        val one: FetchOne<Item>? = if (fetchOne) ::fetch else null
        EntitySync(store, SyncEntity.ALARMS, one).follow(bus, scope)
        return store
    }

    private fun fetch(id: String): Outcome<Item> {
        fetched += id
        if (fetchFails) return Outcome.Failure(RemoteFailure.Timeout)
        return Outcome.Success(Item(id, 0))
    }

    @Test
    fun anUpsertFetchesOnlyThatEntityWithoutReloadingTheList() = runTest {
        val store = followed()
        bus.flow.emit(SyncSignal.Changed(SyncEntity.ALARMS, SyncOp.UPSERT, "c"))
        assertEquals(listOf("c"), fetched)
        assertEquals(listOf("c", "a", "b"), store.items.value.map { it.id })
        assertEquals(1, lists)
    }

    @Test
    fun aDeleteRemovesLocallyWithoutAnyRequest() = runTest {
        val store = followed()
        bus.flow.emit(SyncSignal.Changed(SyncEntity.ALARMS, SyncOp.DELETE, "a"))
        assertEquals(listOf("b"), store.items.value.map { it.id })
        assertTrue(fetched.isEmpty())
        assertEquals(1, lists)
    }

    @Test
    fun otherEntitiesAreIgnored() = runTest {
        val store = followed()
        bus.flow.emit(SyncSignal.Changed(SyncEntity.TIMERS, SyncOp.DELETE, "a"))
        assertEquals(2, store.items.value.size)
        assertEquals(1, lists)
    }

    @Test
    fun withoutSingleFetchOrIdTheListIsReloaded() = runTest {
        followed(fetchOne = false)
        bus.flow.emit(SyncSignal.Changed(SyncEntity.ALARMS, SyncOp.UPSERT, "c"))
        bus.flow.emit(SyncSignal.Changed(SyncEntity.ALARMS, SyncOp.UPSERT, null))
        assertEquals(3, lists)
    }

    @Test
    fun aFailedSingleFetchFallsBackToTheList() = runTest {
        followed()
        fetchFails = true
        bus.flow.emit(SyncSignal.Changed(SyncEntity.ALARMS, SyncOp.UPSERT, "c"))
        assertEquals(2, lists)
    }

    @Test
    fun staleSignalReloadsTheList() = runTest {
        followed()
        bus.flow.emit(SyncSignal.Stale)
        assertEquals(2, lists)
    }

    @Test
    fun stalenessStartsAtTheThreshold() {
        val policy = StalenessPolicy()
        assertFalse(policy.isStale(4.minutes))
        assertTrue(policy.isStale(5.minutes))
    }

    @Test
    fun wireNamesParse() {
        assertEquals(SyncEntity.REMINDERS, SyncEntity.parse(" reminders "))
        assertEquals(SyncOp.DELETE, SyncOp.parse("delete"))
        assertEquals(null, SyncEntity.parse("secrets"))
    }
}
