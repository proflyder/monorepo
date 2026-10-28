package dev.proflyder.home.core

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    dev.proflyder.home.core.endpoint.ServerEndpointTest::class,
    dev.proflyder.home.core.time.DurationTextTest::class,
    dev.proflyder.home.core.time.TimeTextTest::class,
    dev.proflyder.home.core.nav.PendingTransitionsTest::class,
    dev.proflyder.home.core.nav.TabRouteStackTest::class,
)
class AllCoreTests
