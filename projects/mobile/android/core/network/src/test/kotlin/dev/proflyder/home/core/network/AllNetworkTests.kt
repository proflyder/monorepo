package dev.proflyder.home.core.network

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    ConnectSchedulerClientTest::class,
    SchedulerJsonTest::class,
    ControlCatalogClientTest::class,
)
class AllNetworkTests
