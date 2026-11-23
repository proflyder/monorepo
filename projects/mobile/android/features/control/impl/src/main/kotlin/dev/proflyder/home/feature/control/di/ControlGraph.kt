package dev.proflyder.home.feature.control.di

import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.data.FeatureScope
import dev.proflyder.home.core.runtime.FeatureScopes
import dev.proflyder.home.feature.control.ControlRepository
import dev.proflyder.home.feature.control.VoiceProfileSwitch
import dev.proflyder.home.feature.control.ControlScope
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesTo
import dev.zacsweers.metro.GraphExtension
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.Provides
import dev.zacsweers.metro.SingleIn

@GraphExtension(ControlScope::class)
interface ControlGraph {
    val repository: ControlRepository
    val voiceProfile: VoiceProfileSwitch

    @ContributesTo(AppScope::class)
    @GraphExtension.Factory
    fun interface Factory {
        fun createControlGraph(@Provides lifetime: FeatureLifetime): ControlGraph
    }
}

@SingleIn(AppScope::class)
@Inject
class ControlFeatureScope internal constructor(
    factory: ControlGraph.Factory,
    scopes: FeatureScopes,
) : FeatureScope<ControlGraph> by scopes.create(factory::createControlGraph)
