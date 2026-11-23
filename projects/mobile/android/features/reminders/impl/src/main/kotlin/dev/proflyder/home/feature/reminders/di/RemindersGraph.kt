package dev.proflyder.home.feature.reminders.di

import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.data.FeatureScope
import dev.proflyder.home.core.data.map
import dev.proflyder.home.core.runtime.FeatureScopes
import dev.proflyder.home.feature.reminders.ReminderRepository
import dev.proflyder.home.feature.reminders.data.ConnectReminderRemote
import dev.proflyder.home.feature.reminders.data.ReminderRemote
import dev.proflyder.home.feature.reminders.data.StoreReminderRepository
import dev.proflyder.home.feature.reminders.domain.SaveReminder
import dev.proflyder.home.feature.reminders.RemindersScope
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.BindingContainer
import dev.zacsweers.metro.Binds
import dev.zacsweers.metro.ContributesTo
import dev.zacsweers.metro.GraphExtension
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.Provides
import dev.zacsweers.metro.SingleIn

@GraphExtension(RemindersScope::class)
interface RemindersGraph {
    val repository: ReminderRepository
    val saveReminder: SaveReminder

    @ContributesTo(AppScope::class)
    @GraphExtension.Factory
    fun interface Factory {
        fun createRemindersGraph(@Provides lifetime: FeatureLifetime): RemindersGraph
    }
}

@ContributesTo(RemindersScope::class)
@BindingContainer
abstract class RemindersBindings {
    @Binds
    internal abstract fun repository(impl: StoreReminderRepository): ReminderRepository

    @Binds
    internal abstract fun remote(impl: ConnectReminderRemote): ReminderRemote
}

@SingleIn(AppScope::class)
@Inject
class RemindersFeatureScope internal constructor(
    factory: RemindersGraph.Factory,
    scopes: FeatureScopes,
) : FeatureScope<RemindersGraph> by scopes.create(factory::createRemindersGraph)

@ContributesTo(AppScope::class)
@BindingContainer
object RemindersExports {
    @Provides
    @SingleIn(AppScope::class)
    fun repositoryScope(scope: RemindersFeatureScope): FeatureScope<ReminderRepository> = scope.map { it.repository }
}
