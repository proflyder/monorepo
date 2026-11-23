package dev.proflyder.home.feature.push

import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.data.LoadState
import dev.proflyder.home.core.data.PagingAction
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.data.SyncEntity
import dev.proflyder.home.core.data.SyncOp
import dev.proflyder.home.core.data.SyncSignal
import dev.proflyder.home.core.data.SyncSignals
import dev.proflyder.home.core.model.ConnectCode
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
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
@Suppress("LongLambda")
class SchedulerNotificationHistoryRepositoryTest {
    @Test
    fun theFirstPageFillsTheListAndTheUnreadBadge() = runTest {
        val api = FakeApi(entries = listOf(entry("a", read = false), entry("b", read = true)))
        withRepository(api) { repository ->
            advanceUntilIdle()

            assertEquals(listOf("a", "b"), repository.paged.state.value.items.map { it.key })
            assertEquals(1, repository.unread.value)
            assertNull(api.requestedCursor)
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
            assertEquals(false, repository.paged.state.value.item("a")?.read)
        }
    }

    @Test
    fun markingReadKeepsTheServerStateWhenTheCallSucceeds() = runTest {
        val api = FakeApi(entries = listOf(entry("a", read = false)))
        withRepository(api) { repository ->
            advanceUntilIdle()

            val result = repository.setRead(entry("a", read = false), true)

            assertTrue(result is Outcome.Success)
            assertEquals(true, repository.paged.state.value.item("a")?.read)
            assertEquals(0, repository.unread.value)
        }
    }

    @Test
    fun aHiddenDeleteIsFilteredFromARefresh() = runTest {
        val api = FakeApi(entries = listOf(entry("a"), entry("b")))
        withRepository(api) { repository ->
            advanceUntilIdle()
            repository.paged.hide("a")

            repository.paged.dispatch(PagingAction.Refresh(RefreshMode.HARD))
            advanceUntilIdle()

            assertEquals(listOf("b"), repository.paged.state.value.items.map { it.key })
        }
    }

    @Test
    fun removingCallsTheServerAndKeepsTheItemHidden() = runTest {
        val api = FakeApi(entries = listOf(entry("a"), entry("b")))
        withRepository(api) { repository ->
            advanceUntilIdle()
            repository.paged.hide("a")

            val result = repository.remove(entry("a"))

            assertTrue(result is Outcome.Success)
            assertEquals(listOf("a"), api.removed)
            assertEquals(listOf("b"), repository.paged.state.value.items.map { it.key })
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
                repository.paged.dispatch(PagingAction.LoadMore)
                advanceUntilIdle()
            }
            assertEquals(60, repository.paged.state.value.items.size)

            api.entries = listOf(entry("fresh")) + entries
            signals.emit(SyncSignal.Changed(SyncEntity.NOTIFICATIONS, SyncOp.UPSERT, "fresh"))
            advanceUntilIdle()

            assertEquals(61, repository.paged.state.value.items.size)
            assertEquals("fresh", repository.paged.state.value.items.first().key)
        }
    }

    @Test
    fun aSyncDeleteRemovesTheEntryWithoutResettingTheList() = runTest {
        val api = FakeApi(entries = listOf(entry("a"), entry("b")))
        val signals = MutableSharedFlow<SyncSignal>()
        withRepository(api, signals) { repository ->
            advanceUntilIdle()

            api.entries = listOf(entry("b"))
            signals.emit(SyncSignal.Changed(SyncEntity.NOTIFICATIONS, SyncOp.DELETE, "a"))
            advanceUntilIdle()

            assertEquals(listOf("b"), repository.paged.state.value.items.map { it.key })
            repository.paged.dispatch(PagingAction.Refresh(RefreshMode.HARD))
            advanceUntilIdle()
            assertFalse(repository.paged.state.value.items.any { it.key == "a" })
        }
    }

    @Test
    fun aMissingDeviceHeaderReadsAsAnEmptyHistoryInsteadOfAnError() = runTest {
        val api = FakeApi(entries = listOf(entry("a")), rejection = ServerError(deviceRequired = true))
        withRepository(api) { repository ->
            advanceUntilIdle()

            assertTrue(repository.paged.state.value.items.isEmpty())
            assertFalse(repository.paged.state.value.refresh is LoadState.Failed)
            assertEquals(0, repository.unread.value)
        }
    }

    @Test
    fun anotherBadRequestStillSurfacesAsAFailure() = runTest {
        val api = FakeApi(entries = listOf(entry("a")), rejection = ServerError(deviceRequired = false))
        withRepository(api) { repository ->
            advanceUntilIdle()

            assertTrue(repository.paged.state.value.items.isEmpty())
            assertTrue(repository.paged.state.value.refresh is LoadState.Failed)
        }
    }

    private suspend fun TestScope.withRepository(
        api: NotificationHistoryApi,
        signals: MutableSharedFlow<SyncSignal> = MutableSharedFlow(),
        block: suspend TestScope.(SchedulerNotificationHistoryRepository) -> Unit,
    ) {
        FeatureLifetime(this).use { lifetime ->
            block(SchedulerNotificationHistoryRepository(
                api,
                lifetime,
                object : SyncSignals {
                    override val signals = signals
                },
            ))
        }
    }

    private class FakeApi(
        var entries: List<NotificationHistory> = emptyList(),
        var failRead: Boolean = false,
        private val rejection: RemoteFailure? = null,
    ) : NotificationHistoryApi {
        var requestedCursor: String? = null
        val removed = mutableListOf<String>()

        override suspend fun page(cursor: String?, limit: Int): Outcome<NotificationHistoryPage> {
            rejection?.let { return Outcome.Failure(it) }
            requestedCursor = cursor
            val start = cursor?.toIntOrNull() ?: 0
            val page = entries.drop(start).take(20)
            val next = start + page.size
            return Outcome.Success(NotificationHistoryPage(page, next.takeIf { it < entries.size }?.toString()))
        }

        override suspend fun unread(): Outcome<Int> =
            rejection?.let { Outcome.Failure(it) } ?: Outcome.Success(entries.count { !it.read })

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
        fun ServerError(deviceRequired: Boolean) = RemoteFailure.Server(
            httpStatus = 400,
            code = ConnectCode.INVALID_ARGUMENT,
            message = if (deviceRequired) "device_required" else "invalid argument",
            reason = if (deviceRequired) "device_required" else "invalid argument",
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
