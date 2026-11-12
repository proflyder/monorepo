package dev.proflyder.home.feature.alarms

import dev.proflyder.home.core.model.RepeatKind
import org.junit.Assert.assertEquals
import org.junit.Test

class AlarmLabelTest {
    private fun draft(label: String) = AlarmDraft(
        label = label,
        hour = 7,
        minute = 30,
        timezone = "Asia/Almaty",
        repeat = RepeatKind.DAILY,
        weekdays = emptyList(),
        enabled = true,
        onceDate = null,
    )

    @Test
    fun emptyAndWhitespaceUseTheDefault() {
        assertEquals("Будильник", draft("").withDefaultLabel("Будильник").label)
        assertEquals("Будильник", draft("   ").withDefaultLabel("Будильник").label)
        assertEquals("Alarm", draft("\n\t ").withDefaultLabel("Alarm").label)
    }

    @Test
    fun typedLabelIsKeptUnchanged() {
        assertEquals("Gym", draft("Gym").withDefaultLabel("Будильник").label)
        assertEquals("  Gym  ", draft("  Gym  ").withDefaultLabel("Будильник").label)
    }
}
