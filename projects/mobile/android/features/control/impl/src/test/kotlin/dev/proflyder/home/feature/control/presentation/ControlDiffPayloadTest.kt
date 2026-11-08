package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.LightControls
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.ui.PayloadSet
import dev.proflyder.home.feature.control.ControlItem
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * The DiffUtil payload is what keeps a card from blinking: every visual change
 * must map to its aspect, so the holder can update in place instead of a full
 * rebind (which the item animator renders as a crossfade).
 */
class ControlDiffPayloadTest {
    private fun entry(
        id: String = "light.desk",
        kind: ControlKind = ControlKind.LIGHT,
        title: String = "Desk lamp",
        icon: String = "lightbulb-outline",
        power: PowerState? = PowerState.OFF,
        available: Boolean = true,
        light: LightControls? = null,
        actions: Set<ControlAction> = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
    ) = ControlEntry(
        id = id,
        kind = kind,
        title = title,
        room = "Office",
        icon = icon,
        actions = actions,
        power = power,
        available = available,
        light = light,
    )

    private fun device(
        entry: ControlEntry = entry(),
        busy: Boolean = false,
        statesKnown: Boolean = true,
    ) = ControlItem.Device(entry, busy, statesKnown)

    private fun profile(
        entry: ControlEntry = entry(id = "switch.voice_profile", kind = ControlKind.PROFILE),
        busy: Boolean = false,
        statesKnown: Boolean = true,
    ) = ControlItem.Profile(entry, busy, statesKnown)

    private fun scenario(
        entry: ControlEntry = entry(id = "script.movie", kind = ControlKind.SCENARIO, power = null),
        busy: Boolean = false,
    ) = ControlItem.Scenario(entry, busy)

    private fun changes(old: ControlItem, new: ControlItem): Set<ControlChange>? =
        (ControlDiff.getChangePayload(old, new) as? PayloadSet<*>)?.aspects?.filterIsInstance<ControlChange>()?.toSet()

    @Test
    fun identicalItemsHaveNoPayload() {
        assertNull(changes(device(), device()))
        assertNull(changes(profile(), profile()))
        assertNull(changes(scenario(), scenario()))
        assertNull(changes(ControlItem.Header("room:Office", "Office", "1 of 2 on"), ControlItem.Header("room:Office", "Office", "1 of 2 on")))
    }

    @Test
    fun devicePowerChangeReportsPowerAndState() {
        assertEquals(
            setOf(ControlChange.POWER, ControlChange.STATE),
            changes(device(entry(power = PowerState.OFF)), device(entry(power = PowerState.ON))),
        )
    }

    @Test
    fun deviceBusyChangeReportsOnlyBusy() {
        assertEquals(setOf(ControlChange.BUSY), changes(device(), device(busy = true)))
    }

    @Test
    fun deviceAvailabilityChangeReportsAvailabilityAndState() {
        assertEquals(
            setOf(ControlChange.AVAILABILITY, ControlChange.STATE),
            changes(device(entry(available = true)), device(entry(available = false))),
        )
    }

    @Test
    fun deviceTitleAndIconChangesAreSeparateAspects() {
        assertEquals(setOf(ControlChange.TITLE), changes(device(), device(entry(title = "Reading lamp"))))
        assertEquals(setOf(ControlChange.ICON), changes(device(), device(entry(icon = "desk-lamp"))))
        // Icon hints are matched case-insensitively.
        assertNull(changes(device(entry(icon = "Desk")), device(entry(icon = "desk"))))
    }

    @Test
    fun deviceBrightnessChangeReportsStateText() {
        val off = entry(power = PowerState.ON, light = LightControls(brightness = true, colorTemp = false, color = false, brightnessPct = 40))
        val bright = entry(power = PowerState.ON, light = LightControls(brightness = true, colorTemp = false, color = false, brightnessPct = 80))
        assertEquals(setOf(ControlChange.STATE), changes(device(off), device(bright)))
    }

    @Test
    fun deviceCombinedChangesAreMergedIntoOnePayload() {
        val old = device(entry(power = PowerState.OFF, available = true, title = "Desk lamp"), busy = false)
        val new = device(entry(power = PowerState.ON, available = false, title = "Reading lamp"), busy = true)
        assertEquals(
            setOf(
                ControlChange.TITLE,
                ControlChange.POWER,
                ControlChange.AVAILABILITY,
                ControlChange.STATE,
                ControlChange.BUSY,
            ),
            changes(old, new),
        )
    }

    @Test
    fun nonVisualChangeHasNoPayload() {
        // Actions are not rendered by the row, so this difference cannot be
        // expressed as a partial update and falls back to a full bind.
        val old = device(entry(actions = setOf(ControlAction.TURN_ON)))
        val new = device(entry(actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF)))
        assertNull(changes(old, new))
    }

    @Test
    fun profileCoversPowerBusyAvailabilityTitleAndIcon() {
        assertEquals(
            setOf(ControlChange.POWER, ControlChange.STATE),
            changes(profile(entry(kind = ControlKind.PROFILE, power = PowerState.OFF)), profile(entry(kind = ControlKind.PROFILE, power = PowerState.ON))),
        )
        assertEquals(setOf(ControlChange.BUSY), changes(profile(), profile(busy = true)))
        assertEquals(
            setOf(ControlChange.AVAILABILITY, ControlChange.STATE),
            changes(profile(), profile(entry(kind = ControlKind.PROFILE, available = false))),
        )
        assertEquals(setOf(ControlChange.TITLE), changes(profile(), profile(entry(kind = ControlKind.PROFILE, title = "Assistant mode"))))
        assertEquals(setOf(ControlChange.ICON), changes(profile(), profile(entry(kind = ControlKind.PROFILE, icon = "account-voice"))))
    }

    @Test
    fun scenarioCoversTitleIconBusyAndAvailability() {
        assertEquals(setOf(ControlChange.TITLE), changes(scenario(), scenario(entry(kind = ControlKind.SCENARIO, power = null, title = "Cinema"))))
        assertEquals(setOf(ControlChange.ICON), changes(scenario(), scenario(entry(kind = ControlKind.SCENARIO, power = null, icon = "movie"))))
        assertEquals(setOf(ControlChange.BUSY), changes(scenario(), scenario(busy = true)))
        assertEquals(
            setOf(ControlChange.AVAILABILITY),
            changes(scenario(), scenario(entry(kind = ControlKind.SCENARIO, power = null, available = false))),
        )
    }

    @Test
    fun headerCoversTitleAndSummary() {
        val base = ControlItem.Header("room:Office", "Office", "1 of 2 on")
        assertEquals(setOf(ControlChange.TITLE), changes(base, ControlItem.Header("room:Office", "Study", "1 of 2 on")))
        assertEquals(setOf(ControlChange.SUMMARY), changes(base, ControlItem.Header("room:Office", "Office", "2 of 2 on")))
        assertEquals(setOf(ControlChange.SUMMARY), changes(base, ControlItem.Header("room:Office", "Office", null)))
    }

    private fun group(
        expanded: Boolean = false,
        power: PowerState = PowerState.OFF,
    ) = ControlItem.DeviceGroup(
        deviceId = "dev.lamp",
        title = "Ceiling lamp",
        primary = listOf(ControlItem.DeviceGroup.Row(entry(power = power), busy = false)),
        settings = emptyList(),
        expanded = expanded,
        statesKnown = true,
    )

    @Test
    fun deviceGroupChangesUseTheGroupPayload() {
        assertNull(changes(group(), group()))
        assertEquals(setOf(ControlChange.GROUP), changes(group(), group(power = PowerState.ON)))
        assertEquals(setOf(ControlChange.GROUP), changes(group(), group(expanded = true)))
    }

    @Test
    fun hubContentChangeUsesAnInPlacePayload() {
        val old = ControlItem.Hub(emptyList(), emptyMap(), nowUnix = 10)
        val new = ControlItem.Hub(emptyList(), emptyMap(), nowUnix = 11)
        assertEquals(setOf(ControlChange.HUB), changes(old, new))
        assertNull(changes(old, ControlItem.Hub(emptyList(), emptyMap(), nowUnix = 10)))
    }

    @Test
    fun severalPayloadsMergeIntoOneAspectSet() {
        val merged = PayloadSet.merge(
            listOf(
                PayloadSet.of(ControlChange.TITLE),
                PayloadSet.of(ControlChange.POWER, ControlChange.STATE),
            ),
            ControlChange::class.java,
        )
        assertEquals(setOf(ControlChange.TITLE, ControlChange.POWER, ControlChange.STATE), merged?.aspects)
        assertNull(PayloadSet.merge(emptyList(), ControlChange::class.java))
        assertNull(PayloadSet.merge(listOf("not a payload"), ControlChange::class.java))
    }
}
