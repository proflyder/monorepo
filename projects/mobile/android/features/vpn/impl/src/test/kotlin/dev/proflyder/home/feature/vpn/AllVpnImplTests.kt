package dev.proflyder.home.feature.vpn

import dev.proflyder.home.feature.vpn.data.StoredVpnProfileTest
import dev.proflyder.home.feature.vpn.presentation.VpnScreenshotTest
import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(VpnScreenshotTest::class, StoredVpnProfileTest::class)
class AllVpnImplTests
