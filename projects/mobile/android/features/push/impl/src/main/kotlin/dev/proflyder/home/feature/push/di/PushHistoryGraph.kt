package dev.proflyder.home.feature.push.di

import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.data.FeatureScope
import dev.proflyder.home.core.data.map
import dev.proflyder.home.core.runtime.FeatureScopes
import dev.proflyder.home.feature.push.NotificationHistoryRepository
import dev.proflyder.home.feature.push.PushHistoryScope
import dev.proflyder.home.feature.push.SchedulerNotificationHistoryRepository
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.BindingContainer
import dev.zacsweers.metro.Binds
import dev.zacsweers.metro.ContributesTo
import dev.zacsweers.metro.GraphExtension
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.Provides
import dev.zacsweers.metro.SingleIn

@GraphExtension(PushHistoryScope::class)
interface PushHistoryGraph {
    val history: NotificationHistoryRepository

    @ContributesTo(AppScope::class)
    @GraphExtension.Factory
    fun interface Factory {
        fun createPushHistoryGraph(@Provides lifetime: FeatureLifetime): PushHistoryGraph
    }
}

@ContributesTo(PushHistoryScope::class)
@BindingContainer
abstract class PushHistoryBindings {
    @Binds
    internal abstract fun history(impl: SchedulerNotificationHistoryRepository): NotificationHistoryRepository
}

@SingleIn(AppScope::class)
@Inject
class PushHistoryFeatureScope internal constructor(
    factory: PushHistoryGraph.Factory,
    scopes: FeatureScopes,
) : FeatureScope<PushHistoryGraph> by scopes.create(factory::createPushHistoryGraph)

@ContributesTo(AppScope::class)
@BindingContainer
object PushHistoryExports {
    @Provides
    @SingleIn(AppScope::class)
    fun historyScope(scope: PushHistoryFeatureScope): FeatureScope<NotificationHistoryRepository> = scope.map { it.history }
}
