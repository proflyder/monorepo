package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.PowerState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class WakeFeedbackTest {
    private fun entry(
        id: String,
        kind: ControlKind = ControlKind.SCENARIO,
        available: Boolean = true,
    ) = ControlEntry(
        id = id,
        kind = kind,
        title = "Script title from server",
        room = "",
        icon = "",
        actions = setOf(ControlAction.RUN),
        power = if (kind == ControlKind.SWITCH) PowerState.OFF else null,
        available = available,
    )

    @Test
    fun bothWakeScriptsBecomeTwoActionsInAStableOrder() {
        val actions = WakeFeedback.actions(
            listOf(
                entry(WakeFeedback.MISSED_ENTRY_ID),
                entry("script.unrelated"),
                entry(WakeFeedback.FALSE_ALARM_ENTRY_ID),
            ),
        )
        assertEquals(
            listOf(WakeFeedbackKind.FALSE_ALARM, WakeFeedbackKind.MISSED),
            actions.map { it.kind },
        )
        assertEquals(
            listOf(WakeFeedback.FALSE_ALARM_ENTRY_ID, WakeFeedback.MISSED_ENTRY_ID),
            actions.map { it.entry.id },
        )
    }

    @Test
    fun onlyAnExposedScriptShowsItsOwnButton() {
        assertEquals(
            listOf(WakeFeedbackKind.MISSED),
            WakeFeedback.actions(listOf(entry(WakeFeedback.MISSED_ENTRY_ID))).map { it.kind },
        )
        assertTrue(WakeFeedback.actions(emptyList()).isEmpty())
        assertTrue(WakeFeedback.actions(listOf(entry("script.unrelated"))).isEmpty())
    }

    @Test
    fun anEntryWithTheRightIdButTheWrongKindIsIgnored() {
        assertTrue(
            WakeFeedback.actions(listOf(entry(WakeFeedback.FALSE_ALARM_ENTRY_ID, ControlKind.SWITCH))).isEmpty(),
        )
    }

    @Test
    fun pendingAndAvailabilityTravelWithTheAction() {
        val actions = WakeFeedback.actions(
            listOf(
                entry(WakeFeedback.FALSE_ALARM_ENTRY_ID, available = false),
                entry(WakeFeedback.MISSED_ENTRY_ID),
            ),
            pending = setOf(WakeFeedback.MISSED_ENTRY_ID),
        )
        assertEquals(false, actions.first { it.kind == WakeFeedbackKind.FALSE_ALARM }.entry.available)
        assertEquals(true, actions.first { it.kind == WakeFeedbackKind.MISSED }.busy)
    }
}
