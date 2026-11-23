package dev.proflyder.home.feature.scenarios.di

import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.data.FeatureScope
import dev.proflyder.home.core.data.map
import dev.proflyder.home.core.runtime.FeatureScopes
import dev.proflyder.home.feature.scenarios.ScenarioRepository
import dev.proflyder.home.feature.scenarios.data.ConnectScenarioRemote
import dev.proflyder.home.feature.scenarios.data.ScenarioRemote
import dev.proflyder.home.feature.scenarios.data.StoreScenarioRepository
import dev.proflyder.home.feature.scenarios.domain.DeleteScenario
import dev.proflyder.home.feature.scenarios.domain.LoadScenario
import dev.proflyder.home.feature.scenarios.domain.RunScenario
import dev.proflyder.home.feature.scenarios.domain.SaveScenario
import dev.proflyder.home.feature.scenarios.domain.SetScenarioEnabled
import dev.proflyder.home.feature.scenarios.ScenariosScope
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.BindingContainer
import dev.zacsweers.metro.Binds
import dev.zacsweers.metro.ContributesTo
import dev.zacsweers.metro.GraphExtension
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.Provides
import dev.zacsweers.metro.SingleIn

@GraphExtension(ScenariosScope::class)
interface ScenariosGraph {
    val repository: ScenarioRepository
    val loadScenario: LoadScenario
    val saveScenario: SaveScenario
    val deleteScenario: DeleteScenario
    val setScenarioEnabled: SetScenarioEnabled
    val runScenario: RunScenario

    @ContributesTo(AppScope::class)
    @GraphExtension.Factory
    fun interface Factory {
        fun createScenariosGraph(@Provides lifetime: FeatureLifetime): ScenariosGraph
    }
}

@ContributesTo(ScenariosScope::class)
@BindingContainer
abstract class ScenariosBindings {
    @Binds
    internal abstract fun repository(impl: StoreScenarioRepository): ScenarioRepository

    @Binds
    internal abstract fun remote(impl: ConnectScenarioRemote): ScenarioRemote
}

@SingleIn(AppScope::class)
@Inject
class ScenariosFeatureScope internal constructor(
    factory: ScenariosGraph.Factory,
    scopes: FeatureScopes,
) : FeatureScope<ScenariosGraph> by scopes.create(factory::createScenariosGraph)

@ContributesTo(AppScope::class)
@BindingContainer
object ScenariosExports {
    @Provides
    @SingleIn(AppScope::class)
    fun repositoryScope(scope: ScenariosFeatureScope): FeatureScope<ScenarioRepository> = scope.map { it.repository }
}
