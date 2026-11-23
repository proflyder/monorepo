package dev.proflyder.home.feature.alarms.presentation

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    AlarmCardModeTest::class,
    AlarmSelectionActionsTest::class,
    AlarmSelectionAnimationTest::class,
    AlarmEditorDefaultsTest::class,
    AlarmQuickOffsetStateTest::class,
    AlarmRepeatRulesTest::class,
    AlarmsViewModelTest::class,
    AlarmsActionsViewModelTest::class,
    AlarmsScopeTest::class,
    UpcomingAlarmRulesTest::class,
    AlarmEditorViewModelTest::class,
    dev.proflyder.home.feature.alarms.domain.SaveAlarmLabelTest::class,
)
class AllAlarmImplTests
