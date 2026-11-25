package dev.proflyder.home.feature.hermes

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    HermesRowsTest::class,
    HermesViewModelTest::class,
    HermesScreenshotTest::class,
)
class AllHermesTests
