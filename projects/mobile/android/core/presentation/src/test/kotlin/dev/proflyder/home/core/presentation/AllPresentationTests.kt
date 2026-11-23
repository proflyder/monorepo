package dev.proflyder.home.core.presentation

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    StateViewModelTest::class,
    LatestLauncherTest::class,
    AppEventBusTest::class,
    SwitchBinderTest::class,
    PillSegmentsBinderTest::class,
    ItemDiffTest::class,
    ListAdapterAnimationTest::class,
    ListRefresherTest::class,
    EditorDraftSessionTest::class,
    BindersTest::class,
    ScreenInfrastructureTest::class,
    ScreenLifecycleTest::class,
    ScreenEventsTest::class,
    SheetsScreenshotTest::class,
    ScreenResultsTest::class,
    ScreenScopesTest::class,
    UndoableRemovalTest::class,
    PagingHandlerTest::class,
)
class AllPresentationTests
