package dev.proflyder.home.core.runtime

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    TickerFlowTest::class,
    BackoffTest::class,
    ProcessAppForegroundTest::class,
    DeviceIdentityTest::class,
    ObservabilityTest::class,
)
class RuntimeTests
