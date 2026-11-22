package dev.proflyder.home.core.presentation

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    StateViewModelTest::class,
    LatestLauncherTest::class,
    AppEventBusTest::class,
    SwitchBinderTest::class,
    ItemDiffTest::class,
    ListRefresherTest::class,
    EditorDraftSessionTest::class,
    BindersTest::class,
)
class AllPresentationTests
