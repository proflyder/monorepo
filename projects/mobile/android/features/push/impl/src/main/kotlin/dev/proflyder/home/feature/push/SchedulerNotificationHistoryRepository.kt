package dev.proflyder.home.feature.push

import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.data.Page
import dev.proflyder.home.core.data.PageSource
import dev.proflyder.home.core.data.Pager
import dev.proflyder.home.core.data.PagerRemoval
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
import java.util.Collections
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
    private val pendingDeletes: MutableSet<String> = Collections.synchronizedSet(mutableSetOf())

    override val pager: Pager<NotificationHistory> = Pager(
        scope,
        PageSource { cursor, limit -> page(cursor, limit) },
        NotificationHistory::id,
    )

    override val unread: StateFlow<Int> = mutableUnread.asStateFlow()

    init {
        follow(signals)
        refresh()
    }

    override fun refresh() {
        pager.refresh(RefreshMode.SOFT)
        scope.launch { refreshUnread() }
    }

    override fun removeLocal(entry: NotificationHistory): PagerRemoval<NotificationHistory>? {
        pendingDeletes += entry.id
        return pager.removeLocal(entry.id)
    }

    override fun restoreLocal(removal: PagerRemoval<NotificationHistory>) {
        pendingDeletes -= removal.item.id
        pager.restoreLocal(removal)
        scope.launch { refreshUnread() }
    }

    override suspend fun setRead(entry: NotificationHistory, read: Boolean): Outcome<Unit> =
        pager.optimisticUpdate(
            entry.id,
            { it.copy(read = read) },
            { remote.setRead(entry.id, read) },
        ).also { result -> if (result is Outcome.Success) refreshUnread() }

    override suspend fun remove(entry: NotificationHistory): Outcome<Unit> {
        val result = pager.optimisticRemove(entry.id, { remote.remove(entry.id).map { } })
        pendingDeletes -= entry.id
        if (result is Outcome.Success) {
            refreshUnread()
        } else {
            refresh()
        }
        return result
    }

    override fun removeLater(entry: NotificationHistory) {
        scope.launch { remove(entry) }
    }

    override suspend fun readAll(): Outcome<Unit> =
        pager.optimisticEach(
            { it.copy(read = true) },
            { remote.readAll().map { } },
        ).also { result -> if (result is Outcome.Success) refreshUnread() }

    override suspend fun clear(): Outcome<Unit> {
        val result = remote.clear()
        if (result is Outcome.Success) {
            pager.refresh(RefreshMode.HARD)
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
            pendingDeletes -= id
            pager.removeLocal(id)
            refreshUnread()
            return
        }
        syncReload()
    }

    private suspend fun syncReload() {
        pager.reload()
        refreshUnread()
    }

    private suspend fun refreshUnread() {
        when (val result = remote.unread()) {
            is Outcome.Success -> mutableUnread.value = result.value
            is Outcome.Failure -> if (result.failure.isMissingDevice()) mutableUnread.value = 0
        }
    }

    private suspend fun page(cursor: String?, limit: Int): Outcome<Page<NotificationHistory>> {
        val result = remote.page(cursor, limit)
        if (result is Outcome.Failure && result.failure.isMissingDevice()) {
            return Outcome.Success(Page(emptyList(), null))
        }
        return result.map { Page(it.entries.filterNot { entry -> entry.id in pendingDeletes }, it.nextCursor) }
    }

    private fun RemoteFailure.isMissingDevice(): Boolean =
        this is RemoteFailure.Server && httpStatus == HTTP_BAD_REQUEST

    private companion object {
        const val HTTP_BAD_REQUEST = 400
    }
}
