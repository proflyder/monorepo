package dev.proflyder.home.core.theme

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    ThemeRegistryTest::class,
    ThemeOverlayConsistencyTest::class,
    ThemeNamesTest::class,
    ThemeEffectsTest::class,
    AppearanceControllerTest::class,
)
class ThemeTestSuite
