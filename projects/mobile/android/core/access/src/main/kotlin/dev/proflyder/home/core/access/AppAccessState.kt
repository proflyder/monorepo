package dev.proflyder.home.core.access

import dev.proflyder.home.core.model.AppAccess
import dev.proflyder.home.core.model.AppModule
import kotlinx.coroutines.flow.StateFlow

sealed interface AppAccessState {
    data object Unknown : AppAccessState

    data class Ready(val access: AppAccess) : AppAccessState
}

interface AppAccessSource {
    val state: StateFlow<AppAccessState>
}

val AppAccessState.administrator: Boolean
    get() = (this as? AppAccessState.Ready)?.access?.administrator == true

fun AppAccessState.allows(module: AppModule): Boolean = when (this) {
    AppAccessState.Unknown -> true
    is AppAccessState.Ready -> access.allows(module)
}

val AppAccessState.owner: Boolean
    get() = (this as? AppAccessState.Ready)?.access?.owner == true

val AppAccessState.secretsAllowed: Boolean
    get() = owner && allows(AppModule.SECRETS)
