package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.VisibleWhen
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ConditionalVisibilityTest {
    private val texts = ControlTexts(
        otherDevices = "Other",
        scenarios = "Scenarios",
        unavailable = "Unavailable",
        voice = "Voice",
        onSummary = { on, total -> "$on of $total on" },
    )

    private fun entry(
        id: String,
        kind: ControlKind,
        power: PowerState?,
        visibleWhen: VisibleWhen? = null,
    ) = ControlEntry(
        id = id,
        kind = kind,
        title = id,
        room = "Office",
        icon = "",
        actions = if (kind == ControlKind.SWITCH) {
            setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF)
        } else {
            setOf(ControlAction.PRESS)
        },
        power = power,
        available = true,
        visibleWhen = visibleWhen,
    )

    private fun build(basePower: PowerState?): List<ControlItem> = ControlItems.build(
        entries = listOf(
            entry("switch.komp", ControlKind.SWITCH, basePower),
            entry(
                "toggle.komp_lock",
                ControlKind.SWITCH,
                PowerState.ON,
                VisibleWhen(entry = "switch.komp", state = "on"),
            ),
        ),
        pending = emptySet(),
        statesOk = true,
        filter = ControlFilter.All,
        texts = texts,
    )

    private fun ids(items: List<ControlItem>): List<String> = items.flatMap { item ->
        when (item) {
            is ControlItem.Device -> listOf(item.entry.id)
            is ControlItem.DeviceGroup -> item.primary.map { it.entry.id }
            else -> emptyList()
        }
    }

    @Test
    fun aConditionalEntryIsHiddenWhileItsBaseIsOffOrUnknown() {
        val off = entry("switch.komp", ControlKind.SWITCH, PowerState.OFF)
        val unknown = entry("switch.komp", ControlKind.SWITCH, null)
        val lock = entry(
            "toggle.komp_lock",
            ControlKind.SWITCH,
            PowerState.ON,
            VisibleWhen(entry = "switch.komp", state = "on"),
        )
        assertFalse(lock.isVisible(listOf(off, lock)))
        assertFalse(lock.isVisible(listOf(unknown, lock)))
        assertTrue(lock.isVisible(listOf(entry("switch.komp", ControlKind.SWITCH, PowerState.ON), lock)))
    }

    @Test
    fun theLockToggleAppearsAndDisappearsWithTheCardMainSwitch() {
        val hidden = build(PowerState.OFF)
        assertEquals(listOf("switch.komp"), ids(hidden))
        val shown = build(PowerState.ON)
        assertEquals(listOf("switch.komp", "toggle.komp_lock"), ids(shown))
    }
}
