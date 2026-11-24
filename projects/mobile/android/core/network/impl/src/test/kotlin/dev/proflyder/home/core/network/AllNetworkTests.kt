package dev.proflyder.home.core.network

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    ConnectSchedulerClientTest::class,
    SchedulerJsonTest::class,
    ControlCatalogClientTest::class,
    ConnectScenarioClientTest::class,
    ConnectIntercomClientTest::class,
    ScenarioJsonTest::class,
    ConnectErrorLocalizationTest::class,
    OutcomeCallAdapterTest::class,
    CallRetryTest::class,
    TokenRefreshAuthenticatorTest::class,
    WireDtosTest::class,
    SystemDashboardClientTest::class,
    ChatClientTest::class,
    VpnProfileClientTest::class,
    NotificationHistoryClientTest::class,
    ConnectionHealthTest::class,
    RouteGateTest::class,
    AccessClientTest::class,
    SecretsClientTest::class,
)
class AllNetworkTests
