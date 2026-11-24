package dev.proflyder.home.feature.access

import dev.proflyder.home.feature.access.presentation.AccessScreenshotTest
import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    AccessViewModelTest::class,
    AccessScreenshotTest::class,
)
class AllAccessTests
