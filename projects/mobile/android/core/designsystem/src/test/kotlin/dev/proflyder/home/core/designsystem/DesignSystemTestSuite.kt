package dev.proflyder.home.core.designsystem

import dev.proflyder.home.core.designsystem.glass.GlassChromeTest
import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    ButtonStylesTest::class,
    ButtonRowTest::class,
    GlassChromeTest::class,
    PopupPlacementTest::class,
    PopupStylesTest::class,
    PopupScreenshotTest::class,
    RowRippleTest::class,
    FabStylesTest::class,
    CatalogScreenshotTest::class,
    LightParamsEditorScreenshotTest::class,
    RevealBinderTest::class,
    SectionsTest::class,
    FullWidthRowsTest::class,
    SliderStylesTest::class,
    StickyHeadersLayoutManagerTest::class,
)
class DesignSystemTestSuite
