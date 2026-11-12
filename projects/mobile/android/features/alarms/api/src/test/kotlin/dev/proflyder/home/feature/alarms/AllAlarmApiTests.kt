package dev.proflyder.home.feature.alarms

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    AlarmValidationTest::class,
    AlarmLabelTest::class,
    AlarmEditorDraftCodecTest::class,
    AlarmSchedulingTest::class,
    AlarmEditorTimeTest::class,
    AlarmDisableChoiceTest::class,
)
class AllAlarmApiTests
