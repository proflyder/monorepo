package dev.proflyder.home.feature.push

import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.data.Page
import dev.proflyder.home.core.data.PageSource
import dev.proflyder.home.core.data.Pager
import dev.proflyder.home.core.data.PagerRemoval
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.data.SyncEntity
import dev.proflyder.home.core.data.SyncSignal
import dev.proflyder.home.core.data.SyncSignals
import dev.proflyder.home.core.model.NotificationHistory
import dev.proflyder.home.core.model.Outcome
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
internal class SchedulerNotificationHistoryRepository internal constructor(
    private val remote: NotificationHistoryApi,
    override val pager: Pager<NotificationHistory>,
    private val scope: CoroutineScope,
) : NotificationHistoryRepository {
    private val mutableUnread = MutableStateFlow(0)

    override val unread: StateFlow<Int> = mutableUnread.asStateFlow()

    @Inject
    constructor(
        remote: NotificationHistoryApi,
        lifetime: FeatureLifetime,
        signals: SyncSignals,
    ) : this(
        remote,
        Pager(
            lifetime.scope,
            PageSource { cursor, limit -> remote.page(cursor, limit).map { Page(it.entries, it.nextCursor) } },
            NotificationHistory::id,
        ),
        lifetime.scope,
    ) {
        follow(signals)
        refresh()
    }

    override fun refresh() {
        pager.refresh(RefreshMode.SOFT)
        scope.launch { refreshUnread() }
    }

    override fun removeLocal(entry: NotificationHistory): PagerRemoval<NotificationHistory>? =
        pager.removeLocal(entry.id)

    override fun restoreLocal(removal: PagerRemoval<NotificationHistory>) {
        pager.restoreLocal(removal)
        scope.launch { refreshUnread() }
    }

    override suspend fun setRead(entry: NotificationHistory, read: Boolean): Outcome<Unit> =
        pager.optimistic(
            { items -> items.map { if (it.id == entry.id) it.copy(read = read) else it } },
            { remote.setRead(entry.id, read) },
        ).also { result -> if (result is Outcome.Success) refreshUnread() }

    override suspend fun remove(entry: NotificationHistory): Outcome<Unit> =
        pager.optimistic(
            { items -> items.filterNot { it.id == entry.id } },
            { remote.remove(entry.id).map { } },
        ).also { result -> if (result is Outcome.Success) refreshUnread() }

    override fun removeLater(entry: NotificationHistory) {
        scope.launch { remove(entry) }
    }

    override suspend fun readAll(): Outcome<Unit> =
        pager.optimistic(
            { items -> items.map { it.copy(read = true) } },
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
            .onEach { refresh() }
            .launchIn(scope)
    }

    private suspend fun refreshUnread() {
        when (val result = remote.unread()) {
            is Outcome.Success -> mutableUnread.value = result.value
            is Outcome.Failure -> Unit
        }
    }
}
