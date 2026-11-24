package dev.proflyder.home.feature.update.data

import dev.proflyder.home.feature.update.AvailableUpdate
import dev.proflyder.home.feature.update.UpdateFailureKind
import kotlinx.coroutines.flow.Flow

internal sealed interface UpdateResult<out T> {
    data class Success<T>(val value: T) : UpdateResult<T>

    data class Failure(val kind: UpdateFailureKind) : UpdateResult<Nothing>
}

internal sealed interface UpdateEvent {
    data class Downloading(val percent: Int?) : UpdateEvent

    data object Downloaded : UpdateEvent

    data object Completed : UpdateEvent

    data object Cancelled : UpdateEvent

    data class Failed(val kind: UpdateFailureKind) : UpdateEvent
}

internal interface AppUpdater {
    val supported: Boolean

    suspend fun signIn(): UpdateResult<Unit>

    suspend fun check(): UpdateResult<AvailableUpdate>

    fun update(): Flow<UpdateEvent>
}
