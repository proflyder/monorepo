package dev.proflyder.home

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    MainActivitySmokeTest::class,
    ConnectionSettingsClearTest::class,
)
class AppSmokeTestSuite
