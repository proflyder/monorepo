package dev.proflyder.home.feature.timers.di

import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.data.FeatureScope
import dev.proflyder.home.core.data.map
import dev.proflyder.home.core.runtime.FeatureScopes
import dev.proflyder.home.feature.timers.TimerRepository
import dev.proflyder.home.feature.timers.data.ConnectTimerRemote
import dev.proflyder.home.feature.timers.data.StoreTimerRepository
import dev.proflyder.home.feature.timers.data.TimerRemote
import dev.proflyder.home.feature.timers.domain.StartTimer
import dev.proflyder.home.feature.timers.TimersScope
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.BindingContainer
import dev.zacsweers.metro.Binds
import dev.zacsweers.metro.ContributesTo
import dev.zacsweers.metro.GraphExtension
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.Provides
import dev.zacsweers.metro.SingleIn

@GraphExtension(TimersScope::class)
interface TimersGraph {
    val repository: TimerRepository
    val startTimer: StartTimer

    @ContributesTo(AppScope::class)
    @GraphExtension.Factory
    fun interface Factory {
        fun createTimersGraph(@Provides lifetime: FeatureLifetime): TimersGraph
    }
}

@ContributesTo(TimersScope::class)
@BindingContainer
abstract class TimersBindings {
    @Binds
    internal abstract fun repository(impl: StoreTimerRepository): TimerRepository

    @Binds
    internal abstract fun remote(impl: ConnectTimerRemote): TimerRemote
}

@SingleIn(AppScope::class)
@Inject
class TimersFeatureScope internal constructor(
    factory: TimersGraph.Factory,
    scopes: FeatureScopes,
) : FeatureScope<TimersGraph> by scopes.create(factory::createTimersGraph)

@ContributesTo(AppScope::class)
@BindingContainer
object TimersExports {
    @Provides
    @SingleIn(AppScope::class)
    fun repositoryScope(scope: TimersFeatureScope): FeatureScope<TimerRepository> = scope.map { it.repository }
}
