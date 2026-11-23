package dev.proflyder.home.feature.alarms.di

import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.data.FeatureScope
import dev.proflyder.home.core.runtime.FeatureScopes
import dev.proflyder.home.feature.alarms.AlarmRepository
import dev.proflyder.home.feature.alarms.data.AlarmRemote
import dev.proflyder.home.feature.alarms.data.ConnectAlarmRemote
import dev.proflyder.home.feature.alarms.data.StoreAlarmRepository
import dev.proflyder.home.feature.alarms.domain.SaveAlarm
import dev.proflyder.home.feature.alarms.domain.SnoozeAlarm
import dev.proflyder.home.feature.alarms.AlarmsScope
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.BindingContainer
import dev.zacsweers.metro.Binds
import dev.zacsweers.metro.ContributesTo
import dev.zacsweers.metro.GraphExtension
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.Provides
import dev.zacsweers.metro.SingleIn

@GraphExtension(AlarmsScope::class)
interface AlarmsGraph {
    val repository: AlarmRepository
    val saveAlarm: SaveAlarm
    val snoozeAlarm: SnoozeAlarm

    @ContributesTo(AppScope::class)
    @GraphExtension.Factory
    fun interface Factory {
        fun createAlarmsGraph(@Provides lifetime: FeatureLifetime): AlarmsGraph
    }
}

@ContributesTo(AlarmsScope::class)
@BindingContainer
abstract class AlarmsBindings {
    @Binds
    internal abstract fun repository(impl: StoreAlarmRepository): AlarmRepository

    @Binds
    internal abstract fun remote(impl: ConnectAlarmRemote): AlarmRemote
}

@SingleIn(AppScope::class)
@Inject
class AlarmsFeatureScope internal constructor(
    factory: AlarmsGraph.Factory,
    scopes: FeatureScopes,
) : FeatureScope<AlarmsGraph> by scopes.create(factory::createAlarmsGraph)
