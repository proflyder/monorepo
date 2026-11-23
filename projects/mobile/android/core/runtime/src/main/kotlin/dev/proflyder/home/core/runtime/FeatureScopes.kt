package dev.proflyder.home.core.runtime

import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.data.FeatureScope
import dev.proflyder.home.core.data.RefCountedScope
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.map

@SingleIn(AppScope::class)
@Inject
class FeatureScopes(
    private val appScope: CoroutineScope,
    private val foreground: AppForeground,
) {
    fun <G : Any> create(create: (FeatureLifetime) -> G): FeatureScope<G> =
        RefCountedScope(appScope, foreground.state.map { !it.foreground }, RELEASE_DELAY, create)

    private companion object {
        val RELEASE_DELAY = 5.seconds
    }
}
