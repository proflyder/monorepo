package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.NotificationHistoryPage
import dev.proflyder.home.core.model.Outcome

interface NotificationHistoryApi {
    suspend fun page(cursor: String?, limit: Int): Outcome<NotificationHistoryPage>

    suspend fun unread(): Outcome<Int>

    suspend fun setRead(id: String, read: Boolean): Outcome<Unit>

    suspend fun readAll(): Outcome<Int>

    suspend fun remove(id: String): Outcome<Boolean>

    suspend fun clear(): Outcome<Int>
}
