package dev.proflyder.home.feature.push

import dev.proflyder.home.core.data.Page
import dev.proflyder.home.core.model.map
import dev.proflyder.home.core.data.Pager
import dev.proflyder.home.core.model.NotificationHistory
import dev.proflyder.home.core.model.NotificationHistoryPage
import dev.proflyder.home.core.model.NotificationHistoryType
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.network.NotificationHistoryApi
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class SchedulerNotificationHistoryRepositoryTest {
    @Test
    fun theFirstPageFillsThePagerAndTheUnreadBadge() = runTest {
        val api = FakeApi(entries = listOf(entry("a", read = false), entry("b", read = true)))
        val repository = repository(api)

        repository.refresh()
        advanceUntilIdle()

        assertEquals(listOf("a", "b"), repository.pager.state.value.items.map { it.id })
        assertEquals(1, repository.unread.value)
        assertEquals(null, api.requestedCursor)
    }

    @Test
    fun markingReadIsOptimisticAndRollsBackOnFailure() = runTest {
        val api = FakeApi(entries = listOf(entry("a", read = false)))
        val repository = repository(api)
        repository.refresh()
        advanceUntilIdle()
        api.failRead = true

        val result = repository.setRead(entry("a", read = false), true)

        assertTrue(result is Outcome.Failure)
        assertEquals(false, repository.pager.state.value.items.single().read)
    }

    @Test
    fun markingReadKeepsTheServerStateWhenTheCallSucceeds() = runTest {
        val api = FakeApi(entries = listOf(entry("a", read = false)))
        val repository = repository(api)
        repository.refresh()
        advanceUntilIdle()

        val result = repository.setRead(entry("a", read = false), true)

        assertTrue(result is Outcome.Success)
        assertEquals(true, repository.pager.state.value.items.single().read)
        assertEquals(0, repository.unread.value)
    }

    @Test
    fun aRemovedEntryComesBackAtItsPlaceWhenUndone() = runTest {
        val api = FakeApi(entries = listOf(entry("a"), entry("b"), entry("c")))
        val repository = repository(api)
        repository.refresh()
        advanceUntilIdle()
        val middle = repository.pager.state.value.items[1]

        val removal = checkNotNull(repository.removeLocal(middle))
        assertEquals(listOf("a", "c"), repository.pager.state.value.items.map { it.id })
        repository.restoreLocal(removal)

        assertEquals(listOf("a", "b", "c"), repository.pager.state.value.items.map { it.id })
    }

    @Test
    fun removingAfterTheUndoWindowCallsTheServer() = runTest {
        val api = FakeApi(entries = listOf(entry("a"), entry("b")))
        val repository = repository(api)
        repository.refresh()
        advanceUntilIdle()
        repository.removeLocal(entry("a"))

        val result = repository.remove(entry("a"))

        assertTrue(result is Outcome.Success)
        assertEquals(listOf("a"), api.removed)
        assertEquals(listOf("b"), repository.pager.state.value.items.map { it.id })
    }

    private fun TestScope.repository(api: NotificationHistoryApi): SchedulerNotificationHistoryRepository =
        SchedulerNotificationHistoryRepository(api, pager(api), this)

    private fun TestScope.pager(api: NotificationHistoryApi): Pager<NotificationHistory> =
        Pager(this, { cursor, limit -> api.page(cursor, limit).map { Page(it.entries, it.nextCursor) } }, NotificationHistory::id)

    private class FakeApi(
        var entries: List<NotificationHistory> = emptyList(),
        var failRead: Boolean = false,
    ) : NotificationHistoryApi {
        var requestedCursor: String? = null
        val removed = mutableListOf<String>()

        override suspend fun page(cursor: String?, limit: Int): Outcome<NotificationHistoryPage> {
            requestedCursor = cursor
            return Outcome.Success(NotificationHistoryPage(entries, null))
        }

        override suspend fun unread(): Outcome<Int> = Outcome.Success(entries.count { !it.read })

        override suspend fun setRead(id: String, read: Boolean): Outcome<Unit> {
            if (failRead) return Outcome.Failure(RemoteFailure.Offline("offline"))
            entries = entries.map { if (it.id == id) it.copy(read = read) else it }
            return Outcome.Success(Unit)
        }

        override suspend fun readAll(): Outcome<Int> = Outcome.Success(entries.size)

        override suspend fun remove(id: String): Outcome<Boolean> {
            removed += id
            return Outcome.Success(true)
        }

        override suspend fun clear(): Outcome<Int> = Outcome.Success(entries.size)
    }

    private companion object {
        fun entry(id: String, read: Boolean = false): NotificationHistory = NotificationHistory(
            id = id,
            type = NotificationHistoryType.NOTIFY,
            title = "title",
            body = "body",
            createdUnix = 1_700_000_000,
            read = read,
        )
    }
}
