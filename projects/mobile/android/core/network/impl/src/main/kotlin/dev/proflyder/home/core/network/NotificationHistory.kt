package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.NotificationHistory
import dev.proflyder.home.core.model.NotificationHistoryPage
import dev.proflyder.home.core.model.NotificationHistoryType
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.map
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.serialization.Serializable

@Serializable
internal data class HistoryEntryDto(
    val id: String = "",
    val type: String = "",
    val title: String = "",
    val body: String = "",
    val deepLink: String? = null,
    val createdUnix: String = "0",
    val readUnix: String = "0",
)

@Serializable
internal data class HistoryPageDto(
    val entries: List<HistoryEntryDto> = emptyList(),
    val nextCursor: String? = null,
)

@Serializable
internal data class HistoryEntryResponseDto(val entry: HistoryEntryDto? = null)

@Serializable
internal data class UnreadResponseDto(val unread: Int = 0)

@Serializable
internal data class UpdatedResponseDto(val updated: Int = 0)

@Serializable
internal data class RemovedResponseDto(val removed: Boolean = false)

@Serializable
internal data class RemovedCountDto(val removed: Int = 0)

@Serializable
internal data class ReadBodyDto(val id: String, val read: Boolean)

@Serializable
internal data class IdBodyDto(val id: String)

internal interface NotificationApi {
    @retrofit2.http.GET("v1/push/history")
    suspend fun history(
        @retrofit2.http.Query("cursor") cursor: String?,
        @retrofit2.http.Query("limit") limit: Int,
    ): Outcome<HistoryPageDto>

    @retrofit2.http.GET("v1/push/history/unread-count")
    suspend fun unread(): Outcome<UnreadResponseDto>

    @retrofit2.http.POST("v1/push/history/read")
    suspend fun read(
        @retrofit2.http.Body body: ReadBodyDto,
    ): Outcome<HistoryEntryResponseDto>

    @retrofit2.http.POST("v1/push/history/read-all")
    suspend fun readAll(): Outcome<UpdatedResponseDto>

    @retrofit2.http.POST("v1/push/history/remove")
    suspend fun remove(
        @retrofit2.http.Body body: IdBodyDto,
    ): Outcome<RemovedResponseDto>

    @retrofit2.http.POST("v1/push/history/clear")
    suspend fun clear(): Outcome<RemovedCountDto>
}

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class HttpNotificationHistoryApi(private val http: HomeHttp) : NotificationHistoryApi {
    private val api: NotificationApi by lazy { http.rest(NotificationApi::class, Target.Scheduler) }

    override suspend fun page(cursor: String?, limit: Int): Outcome<NotificationHistoryPage> =
        api.history(cursor, limit.coerceIn(1, MAX_LIMIT)).map { dto ->
            NotificationHistoryPage(dto.entries.map { it.toModel() }, dto.nextCursor?.takeIf(String::isNotBlank))
        }

    override suspend fun unread(): Outcome<Int> = api.unread().map { it.unread }

    override suspend fun setRead(id: String, read: Boolean): Outcome<Unit> =
        api.read(ReadBodyDto(id, read)).map { }

    override suspend fun readAll(): Outcome<Int> = api.readAll().map { it.updated }

    override suspend fun remove(id: String): Outcome<Boolean> = api.remove(IdBodyDto(id)).map { it.removed }

    override suspend fun clear(): Outcome<Int> = api.clear().map { it.removed }

    private fun HistoryEntryDto.toModel(): NotificationHistory = NotificationHistory(
        id = id,
        type = NotificationHistoryType.parse(type),
        title = title,
        body = body,
        deepLink = deepLink?.takeIf(String::isNotBlank),
        createdUnix = createdUnix.toLongOrNull() ?: 0L,
        read = readUnix.toLongOrNull()?.let { it > 0 } ?: false,
    )

    private companion object {
        const val MAX_LIMIT = 50
    }
}
