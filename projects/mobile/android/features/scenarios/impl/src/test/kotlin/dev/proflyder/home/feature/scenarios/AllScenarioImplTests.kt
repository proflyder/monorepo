package dev.proflyder.home.feature.scenarios

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    ScenarioUseCasesTest::class,
    ScenarioEditorViewModelTest::class,
)
class AllScenarioImplTests
