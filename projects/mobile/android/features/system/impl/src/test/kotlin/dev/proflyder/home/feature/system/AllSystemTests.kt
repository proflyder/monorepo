package dev.proflyder.home.feature.system

import dev.proflyder.home.feature.system.presentation.SystemRowsTest
import dev.proflyder.home.feature.system.presentation.SystemScreenshotTest
import dev.proflyder.home.feature.system.presentation.SystemViewModelTest
import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    SystemRowsTest::class,
    SystemViewModelTest::class,
    SystemScreenshotTest::class,
)
class AllSystemTests
