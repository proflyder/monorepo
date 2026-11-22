package dev.proflyder.home.feature.call

import dev.proflyder.home.feature.call.audio.AudioRoutesTest
import dev.proflyder.home.feature.call.audio.CallAudioSessionTest
import dev.proflyder.home.feature.call.presentation.CallActivityRotationTest
import dev.proflyder.home.feature.call.presentation.CallScreenshotTest
import dev.proflyder.home.feature.call.presentation.CallViewModelTest
import dev.proflyder.home.feature.call.session.CallManagerTest
import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    CallManagerTest::class,
    AudioRoutesTest::class,
    CallAudioSessionTest::class,
    CallViewModelTest::class,
    CallActivityRotationTest::class,
    CallScreenshotTest::class,
)
internal class AllCallImplTests
