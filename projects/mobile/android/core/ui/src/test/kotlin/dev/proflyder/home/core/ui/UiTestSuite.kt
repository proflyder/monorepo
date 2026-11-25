package dev.proflyder.home.core.ui

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    BarMotionTest::class,
    ScreenFabsTest::class,
    ForegroundGraceNoticePolicyTest::class,
    SensitiveClipboardLifecycleTest::class,
    SensitiveClipboardTest::class,
)
class UiTestSuite
