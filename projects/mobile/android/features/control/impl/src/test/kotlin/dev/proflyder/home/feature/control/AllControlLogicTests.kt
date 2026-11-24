package dev.proflyder.home.feature.control

import dev.proflyder.home.feature.control.data.CatalogRepositoryTest
import dev.proflyder.home.feature.control.data.OptimisticDeviceStateTest
import dev.proflyder.home.feature.control.domain.SpeakOnStationTest
import dev.proflyder.home.feature.control.presentation.ControlDiffPayloadTest
import dev.proflyder.home.feature.control.presentation.ControlFavoritesTest
import dev.proflyder.home.feature.control.presentation.ControlTapsTest
import dev.proflyder.home.feature.control.presentation.QuickTileAccessTest
import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    ControlItemsTest::class,
    CompactControlsTest::class,
    ConditionalVisibilityTest::class,
    HomeHubTest::class,
    SpeedDialTest::class,
    WakeFeedbackTest::class,
    CatalogRepositoryTest::class,
    OptimisticDeviceStateTest::class,
    SpeakOnStationTest::class,
    ControlDiffPayloadTest::class,
    ControlFavoritesTest::class,
    ControlTapsTest::class,
    QuickTileAccessTest::class,
)
class AllControlLogicTests
