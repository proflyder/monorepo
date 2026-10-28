package dev.proflyder.home.feature.reminders

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    ReminderValidationTest::class,
    ReminderEditorDraftCodecTest::class,
)
class AllReminderApiTests
