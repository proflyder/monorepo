package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.feature.control.ControlFilter
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.core.presentation.ItemChanged
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ControlDiffPayloadTest {
    private fun entry(power: PowerState? = PowerState.OFF, title: String = "Desk lamp") = ControlEntry(
        id = "light.desk",
        kind = ControlKind.LIGHT,
        title = title,
        room = "Office",
        icon = "lightbulb-outline",
        actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
        power = power,
        available = true,
    )

    private fun device(entry: ControlEntry = entry(), busy: Boolean = false) = ControlItem.Device(entry, busy, statesKnown = true)

    @Test
    fun identicalItemsAreNotChanged() {
        assertTrue(ControlDiff.areContentsTheSame(device(), device()))
    }

    @Test
    fun aChangedItemIsUpdatedInPlace() {
        assertEquals(ItemChanged, ControlDiff.getChangePayload(device(), device(entry(PowerState.ON))))
        assertEquals(ItemChanged, ControlDiff.getChangePayload(device(), device(busy = true)))
        assertTrue(ControlDiff.areItemsTheSame(device(), device(entry(title = "Lamp"))))
    }

    @Test
    fun selectingAFilterKeepsTheChipRow() {
        val chips = listOf(ControlItem.FilterChip(ControlFilter.All, "All · 2", 2))
        val before = ControlItem.Filters(chips, ControlFilter.All)
        val after = ControlItem.Filters(chips, ControlFilter.Lights)
        assertTrue(ControlDiff.areItemsTheSame(before, after))
        assertFalse(ControlDiff.areContentsTheSame(before, after))
        assertEquals(ItemChanged, ControlDiff.getChangePayload(before, after))
    }

    @Test
    fun eachItemKindHasItsOwnViewType() {
        val items = listOf(
            ControlItem.Header("room:Office", "Office"),
            ControlItem.Scenario(entry().copy(kind = ControlKind.SCENARIO), busy = false),
            device(),
            ControlItem.Hub(emptyList(), emptyMap(), 0),
            ControlItem.Filters(emptyList(), ControlFilter.All),
        )
        assertEquals(items.size, items.map(ControlViewType::of).toSet().size)
    }
}
