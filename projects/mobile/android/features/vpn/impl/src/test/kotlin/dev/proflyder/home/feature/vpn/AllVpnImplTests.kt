package dev.proflyder.home.feature.vpn

import dev.proflyder.home.feature.vpn.connect.AutoConnectCoordinatorTest
import dev.proflyder.home.feature.vpn.connect.AutoConnectPolicyTest
import dev.proflyder.home.feature.vpn.connect.VpnRouteConditionTest
import dev.proflyder.home.feature.vpn.data.StoredVpnProfileTest
import dev.proflyder.home.feature.vpn.data.VpnProblemsTest
import dev.proflyder.home.feature.vpn.environment.EnvironmentClassifierTest
import dev.proflyder.home.feature.vpn.environment.EnvironmentRulesTest
import dev.proflyder.home.feature.vpn.environment.HomeProbeTest
import dev.proflyder.home.feature.vpn.presentation.VpnScreenshotTest
import dev.proflyder.home.feature.vpn.presentation.VpnToolbarScreenshotTest
import dev.proflyder.home.feature.vpn.transport.TransportProtocolsTest
import dev.proflyder.home.feature.vpn.tunnel.ChainVpnControllerTest
import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    VpnScreenshotTest::class,
    VpnToolbarScreenshotTest::class,
    StoredVpnProfileTest::class,
    TransportProtocolsTest::class,
    EnvironmentRulesTest::class,
    EnvironmentClassifierTest::class,
    HomeProbeTest::class,
    AutoConnectPolicyTest::class,
    AutoConnectCoordinatorTest::class,
    VpnRouteConditionTest::class,
    ChainVpnControllerTest::class,
    VpnProblemsTest::class,
)
class AllVpnImplTests
