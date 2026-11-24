package dev.proflyder.home.core.designsystem

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    ButtonStylesTest::class,
    ButtonRowTest::class,
    PopupPlacementTest::class,
    PopupStylesTest::class,
    PopupScreenshotTest::class,
    RowRippleTest::class,
    GroupedRowsScreenshotTest::class,
    FabStylesTest::class,
    CatalogScreenshotTest::class,
    LightParamsEditorScreenshotTest::class,
    RevealBinderTest::class,
    SectionsTest::class,
    SliderStylesTest::class,
    StickyHeadersScreenshotTest::class,
)
class DesignSystemTestSuite
