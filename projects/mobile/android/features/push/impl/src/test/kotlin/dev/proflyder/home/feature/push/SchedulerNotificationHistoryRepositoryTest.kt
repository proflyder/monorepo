package dev.proflyder.home.feature.push

import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.data.SyncEntity
import dev.proflyder.home.core.data.SyncOp
import dev.proflyder.home.core.data.SyncSignal
import dev.proflyder.home.core.data.SyncSignals
import dev.proflyder.home.core.model.NotificationHistory
import dev.proflyder.home.core.model.NotificationHistoryPage
import dev.proflyder.home.core.model.NotificationHistoryType
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.network.NotificationHistoryApi
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
@Suppress("LongLambda")
class SchedulerNotificationHistoryRepositoryTest {
    @Test
    fun theFirstPageFillsThePagerAndTheUnreadBadge() = runTest {
        val api = FakeApi(entries = listOf(entry("a", read = false), entry("b", read = true)))
        withRepository(api) { repository ->
            advanceUntilIdle()

            assertEquals(listOf("a", "b"), repository.pager.state.value.items.map { it.id })
            assertEquals(1, repository.unread.value)
            assertEquals(null, api.requestedCursor)
        }
    }

    @Test
    fun markingReadIsOptimisticAndRollsBackOnFailure() = runTest {
        val api = FakeApi(entries = listOf(entry("a", read = false)))
        withRepository(api) { repository ->
            advanceUntilIdle()
            api.failRead = true

            val result = repository.setRead(entry("a", read = false), true)

            assertTrue(result is Outcome.Failure)
            assertEquals(false, repository.pager.state.value.items.single().read)
        }
    }

    @Test
    fun markingReadKeepsTheServerStateWhenTheCallSucceeds() = runTest {
        val api = FakeApi(entries = listOf(entry("a", read = false)))
        withRepository(api) { repository ->
            advanceUntilIdle()

            val result = repository.setRead(entry("a", read = false), true)

            assertTrue(result is Outcome.Success)
            assertEquals(true, repository.pager.state.value.items.single().read)
            assertEquals(0, repository.unread.value)
        }
    }

    @Test
    fun aPendingDeleteIsFilteredFromSyncedPagesUntilItIsConfirmed() = runTest {
        val api = FakeApi(entries = listOf(entry("a"), entry("b")))
        withRepository(api) { repository ->
            advanceUntilIdle()
            val target = repository.pager.state.value.items.first { it.id == "a" }

            repository.removeLocal(target)
            repository.pager.refresh(RefreshMode.HARD)
            advanceUntilIdle()

            assertEquals(listOf("b"), repository.pager.state.value.items.map { it.id })
        }
    }

    @Test
    fun undoingAPendingDeleteLetsTheEntryComeBackFromTheServer() = runTest {
        val api = FakeApi(entries = listOf(entry("a"), entry("b")))
        withRepository(api) { repository ->
            advanceUntilIdle()
            val target = repository.pager.state.value.items.first { it.id == "a" }

            val removal = checkNotNull(repository.removeLocal(target))
            repository.restoreLocal(removal)
            repository.pager.refresh(RefreshMode.HARD)
            advanceUntilIdle()

            assertEquals(listOf("a", "b"), repository.pager.state.value.items.map { it.id })
        }
    }

    @Test
    fun removingAfterTheUndoWindowCallsTheServer() = runTest {
        val api = FakeApi(entries = listOf(entry("a"), entry("b")))
        withRepository(api) { repository ->
            advanceUntilIdle()
            repository.removeLocal(entry("a"))

            val result = repository.remove(entry("a"))

            assertTrue(result is Outcome.Success)
            assertEquals(listOf("a"), api.removed)
            assertEquals(listOf("b"), repository.pager.state.value.items.map { it.id })
        }
    }

    @Test
    fun aSyncSignalReloadsOnlyTheFirstPageAndKeepsTheScrollPosition() = runTest {
        val entries = (1..60).map { entry("n-$it") }
        val api = FakeApi(entries = entries)
        val signals = MutableSharedFlow<SyncSignal>()
        withRepository(api, signals) { repository ->
            advanceUntilIdle()
            repeat(2) {
                repository.pager.loadMore()
                advanceUntilIdle()
            }
            assertEquals(60, repository.pager.state.value.items.size)

            api.entries = listOf(entry("fresh")) + entries
            signals.emit(SyncSignal.Changed(SyncEntity.NOTIFICATIONS, SyncOp.UPSERT, "fresh"))
            advanceUntilIdle()

            assertEquals(61, repository.pager.state.value.items.size)
            assertEquals("fresh", repository.pager.state.value.items.first().id)
        }
    }

    @Test
    fun aSyncDeleteRemovesTheEntryWithoutResettingTheList() = runTest {
        val api = FakeApi(entries = listOf(entry("a"), entry("b")))
        val signals = MutableSharedFlow<SyncSignal>()
        withRepository(api, signals) { repository ->
            advanceUntilIdle()

            signals.emit(SyncSignal.Changed(SyncEntity.NOTIFICATIONS, SyncOp.DELETE, "a"))
            advanceUntilIdle()

            assertEquals(listOf("b"), repository.pager.state.value.items.map { it.id })
        }
    }

    @Test
    fun aMissingDeviceHeaderReadsAsAnEmptyHistoryInsteadOfAnError() = runTest {
        val api = FakeApi(entries = listOf(entry("a")), rejectWithoutDevice = true)
        withRepository(api) { repository ->
            advanceUntilIdle()

            assertTrue(repository.pager.state.value.items.isEmpty())
            assertEquals(null, repository.pager.state.value.failure)
            assertEquals(0, repository.unread.value)
        }
    }

    private suspend fun TestScope.withRepository(
        api: NotificationHistoryApi,
        signals: MutableSharedFlow<SyncSignal> = MutableSharedFlow(),
        block: suspend TestScope.(SchedulerNotificationHistoryRepository) -> Unit,
    ) {
        FeatureLifetime(this).use { lifetime ->
            val repository = SchedulerNotificationHistoryRepository(
                api,
                lifetime,
                object : SyncSignals {
                    override val signals = signals
                },
            )
            block(repository)
        }
    }

    private class FakeApi(
        var entries: List<NotificationHistory> = emptyList(),
        var failRead: Boolean = false,
        private val pageSize: Int = 20,
        private val rejectWithoutDevice: Boolean = false,
    ) : NotificationHistoryApi {
        var requestedCursor: String? = null
        val removed = mutableListOf<String>()

        override suspend fun page(cursor: String?, limit: Int): Outcome<NotificationHistoryPage> {
            if (rejectWithoutDevice) return Outcome.Failure(ServerError)
            requestedCursor = cursor
            val start = cursor?.toIntOrNull() ?: 0
            val page = entries.drop(start).take(pageSize)
            val next = start + page.size
            return Outcome.Success(NotificationHistoryPage(page, next.takeIf { it < entries.size }?.toString()))
        }

        override suspend fun unread(): Outcome<Int> =
            if (rejectWithoutDevice) Outcome.Failure(ServerError) else Outcome.Success(entries.count { !it.read })

        override suspend fun setRead(id: String, read: Boolean): Outcome<Unit> {
            if (failRead) return Outcome.Failure(RemoteFailure.Offline("offline"))
            entries = entries.map { if (it.id == id) it.copy(read = read) else it }
            return Outcome.Success(Unit)
        }

        override suspend fun readAll(): Outcome<Int> = Outcome.Success(entries.size)

        override suspend fun remove(id: String): Outcome<Boolean> {
            removed += id
            entries = entries.filterNot { it.id == id }
            return Outcome.Success(true)
        }

        override suspend fun clear(): Outcome<Int> = Outcome.Success(entries.size)
    }

    private companion object {
        val ServerError = RemoteFailure.Server(
            httpStatus = 400,
            code = dev.proflyder.home.core.model.ConnectCode.INVALID_ARGUMENT,
            message = "invalid argument",
        )

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
