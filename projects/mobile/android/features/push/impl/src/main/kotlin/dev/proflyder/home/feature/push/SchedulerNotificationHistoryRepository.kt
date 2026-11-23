package dev.proflyder.home.feature.push

import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.data.ListEdit
import dev.proflyder.home.core.data.Page
import dev.proflyder.home.core.data.PageRequest
import dev.proflyder.home.core.data.PagedList
import dev.proflyder.home.core.data.PagingAction
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.data.SyncEntity
import dev.proflyder.home.core.data.SyncOp
import dev.proflyder.home.core.data.SyncSignal
import dev.proflyder.home.core.data.SyncSignals
import dev.proflyder.home.core.model.NotificationHistory
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.map
import dev.proflyder.home.core.network.NotificationHistoryApi
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch

@SingleIn(PushHistoryScope::class)
@Inject
internal class SchedulerNotificationHistoryRepository(
    private val remote: NotificationHistoryApi,
    lifetime: FeatureLifetime,
    signals: SyncSignals,
) : NotificationHistoryRepository {
    private val scope: CoroutineScope = lifetime.scope
    private val mutableUnread = MutableStateFlow(0)

    override val paged: PagedList<NotificationHistory> = PagedList(scope, this)

    override val unread: StateFlow<Int> = mutableUnread.asStateFlow()

    init {
        follow(signals)
        paged.dispatch(PagingAction.Refresh(RefreshMode.HARD))
        scope.launch { refreshUnread() }
    }

    override suspend fun load(request: PageRequest): Outcome<Page<NotificationHistory>> {
        val result = remote.page(request.cursor, request.size)
        if (result is Outcome.Failure && result.failure.isMissingDevice()) {
            return Outcome.Success(Page(emptyList(), null))
        }
        return result.map { page -> Page(page.entries, page.nextCursor) }
    }

    override suspend fun setRead(entry: NotificationHistory, read: Boolean): Outcome<Unit> {
        val result = paged.commit(
            ListEdit.Update(entry.key) { it.copy(read = read) },
            { remote.setRead(entry.id, read) },
        )
        if (result is Outcome.Success) refreshUnread()
        return result
    }

    override suspend fun remove(entry: NotificationHistory): Outcome<Unit> {
        val result = paged.commit(
            ListEdit.Remove(entry.key),
            { remote.remove(entry.id).map { } },
        )
        if (result is Outcome.Success) refreshUnread()
        return result
    }

    override suspend fun readAll(): Outcome<Unit> {
        val result = paged.commit(
            ListEdit.UpdateAll { it.copy(read = true) },
            { remote.readAll().map { } },
        )
        if (result is Outcome.Success) refreshUnread()
        return result
    }

    override suspend fun clear(): Outcome<Unit> {
        val result = remote.clear()
        if (result is Outcome.Success) {
            paged.dispatch(PagingAction.Refresh(RefreshMode.HARD))
            refreshUnread()
        }
        return result.map { }
    }

    private fun follow(signals: SyncSignals) {
        signals.signals
            .filter { signal -> signal is SyncSignal.Stale || (signal is SyncSignal.Changed && signal.entity == SyncEntity.NOTIFICATIONS) }
            .onEach(::applySignal)
            .launchIn(scope)
    }

    private suspend fun applySignal(signal: SyncSignal) {
        when (signal) {
            SyncSignal.Stale -> syncReload()
            is SyncSignal.Changed -> changed(signal)
        }
    }

    private suspend fun changed(signal: SyncSignal.Changed) {
        val id = signal.id?.takeIf { it.isNotBlank() }
        if (signal.op == SyncOp.DELETE && id != null) {
            paged.hide(id)
            refreshUnread()
            return
        }
        syncReload()
    }

    private suspend fun syncReload() {
        paged.dispatch(PagingAction.Reload)
        refreshUnread()
    }

    private suspend fun refreshUnread() {
        when (val result = remote.unread()) {
            is Outcome.Success -> mutableUnread.value = result.value
            is Outcome.Failure -> if (result.failure.isMissingDevice()) mutableUnread.value = 0
        }
    }

    private fun RemoteFailure.isMissingDevice(): Boolean =
        this is RemoteFailure.Server && httpStatus == HTTP_BAD_REQUEST

    private companion object {
        const val HTTP_BAD_REQUEST = 400
    }
}
