package dev.proflyder.home.feature.secrets.data

import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

@SingleIn(AppScope::class)
@Inject
class SecretsUnlockSession {
    private val unlockedState = MutableStateFlow(false)

    val unlocked: StateFlow<Boolean> = unlockedState.asStateFlow()

    fun unlock() {
        unlockedState.value = true
    }
}
