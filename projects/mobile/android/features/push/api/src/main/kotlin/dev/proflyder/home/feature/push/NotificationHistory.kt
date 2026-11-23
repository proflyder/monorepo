package dev.proflyder.home.feature.push

import dev.proflyder.home.core.data.Pager
import dev.proflyder.home.core.data.PagerRemoval
import dev.proflyder.home.core.model.NotificationHistory
import dev.proflyder.home.core.model.Outcome
import kotlinx.coroutines.flow.StateFlow

typealias NotificationHistoryEntry = NotificationHistory

interface NotificationHistoryRepository {
    val pager: Pager<NotificationHistory>

    val unread: StateFlow<Int>

    fun refresh()

    fun removeLocal(entry: NotificationHistory): PagerRemoval<NotificationHistory>?

    fun restoreLocal(removal: PagerRemoval<NotificationHistory>)

    suspend fun setRead(entry: NotificationHistory, read: Boolean): Outcome<Unit>

    suspend fun remove(entry: NotificationHistory): Outcome<Unit>

    fun removeLater(entry: NotificationHistory)

    suspend fun readAll(): Outcome<Unit>

    suspend fun clear(): Outcome<Unit>
}
