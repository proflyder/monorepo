package dev.proflyder.home.feature.settings.presentation

import dev.proflyder.home.core.data.ListEdit
import dev.proflyder.home.core.data.Page
import dev.proflyder.home.core.data.PageRequest
import dev.proflyder.home.core.data.PagedList
import dev.proflyder.home.core.data.PagedSource
import dev.proflyder.home.core.data.PagingAction
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.model.NotificationHistory
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.feature.push.NotificationHistoryRepository
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

internal class FakeHistoryRepository(initial: List<NotificationHistory>) : NotificationHistoryRepository {
    private var all = initial
    private val scope = CoroutineScope(Dispatchers.Main)
    private val mutableUnread = MutableStateFlow(initial.count { !it.read })

    override val paged: PagedList<NotificationHistory> = PagedList(scope, object : PagedSource<NotificationHistory> {
        override suspend fun load(request: PageRequest): Outcome<Page<NotificationHistory>> =
            Outcome.Success(Page(all, null))
    })

    override suspend fun load(request: PageRequest): Outcome<Page<NotificationHistory>> =
        Outcome.Success(Page(all, null))

    override val unread: StateFlow<Int> = mutableUnread

    init {
        paged.dispatch(PagingAction.Refresh(RefreshMode.HARD))
    }

    override suspend fun setRead(entry: NotificationHistory, read: Boolean): Outcome<Unit> {
        all = all.map { if (it.key == entry.key) it.copy(read = read) else it }
        mutableUnread.value = all.count { !it.read }
        return paged.commit(ListEdit.Update(entry.key) { it.copy(read = read) }) { Outcome.Success(Unit) }
    }

    override suspend fun remove(entry: NotificationHistory): Outcome<Unit> {
        all = all.filterNot { it.key == entry.key }
        mutableUnread.value = all.count { !it.read }
        return paged.commit(ListEdit.Remove(entry.key)) { Outcome.Success(Unit) }
    }

    override suspend fun readAll(): Outcome<Unit> {
        all = all.map { it.copy(read = true) }
        mutableUnread.value = 0
        return paged.commit(ListEdit.UpdateAll { it.copy(read = true) }) { Outcome.Success(Unit) }
    }

    override suspend fun clear(): Outcome<Unit> {
        all = emptyList()
        mutableUnread.value = 0
        paged.dispatch(PagingAction.Refresh(RefreshMode.HARD))
        return Outcome.Success(Unit)
    }
}
