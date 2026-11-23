package dev.proflyder.home.feature.scenarios

import dev.proflyder.home.core.testing.FakeSyncSignals
import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.feature.scenarios.data.ScenarioRemote
import dev.proflyder.home.feature.scenarios.data.StoreScenarioRepository
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

internal fun ScenarioRemote.store(dispatcher: CoroutineDispatcher = Dispatchers.Unconfined): StoreScenarioRepository =
    StoreScenarioRepository(this, FeatureLifetime(CoroutineScope(dispatcher + SupervisorJob())), FakeSyncSignals())
