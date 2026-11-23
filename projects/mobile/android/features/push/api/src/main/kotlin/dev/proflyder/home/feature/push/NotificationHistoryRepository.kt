package dev.proflyder.home.feature.push

import dev.proflyder.home.core.data.PagedList
import dev.proflyder.home.core.data.PagedSource
import dev.proflyder.home.core.model.NotificationHistory
import dev.proflyder.home.core.model.Outcome
import kotlinx.coroutines.flow.StateFlow

interface NotificationHistoryRepository : PagedSource<NotificationHistory> {
    val paged: PagedList<NotificationHistory>

    val unread: StateFlow<Int>

    suspend fun setRead(entry: NotificationHistory, read: Boolean): Outcome<Unit>

    suspend fun remove(entry: NotificationHistory): Outcome<Unit>

    suspend fun readAll(): Outcome<Unit>

    suspend fun clear(): Outcome<Unit>
}
