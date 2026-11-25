package dev.proflyder.home.core.designsystem

import dev.proflyder.home.core.designsystem.list.GroupSlot
import dev.proflyder.home.core.designsystem.list.SectionedAdapter
import dev.proflyder.home.core.designsystem.list.stickyHeaderAtOrBefore
import dev.proflyder.home.core.designsystem.list.slotOf
import org.junit.Assert.assertEquals
import org.junit.Test

class SectionsTest {
    private val headers = setOf(0, 3)
    private val sections = object : SectionedAdapter {
        override fun isSectionHeader(position: Int): Boolean = position in headers
    }

    @Test
    fun slotsMarkTheFirstAndLastRowOfEachGroup() {
        val slots = (0 until 6).map { sections.slotOf(it, 6) }

        assertEquals(
            listOf(
                null,
                GroupSlot(first = true, last = false),
                GroupSlot(first = false, last = true),
                null,
                GroupSlot(first = true, last = false),
                GroupSlot(first = false, last = true),
            ),
            slots,
        )
    }

    @Test
    fun theNearestHeaderAboveARowOwnsIt() {
        assertEquals(listOf(0, 0, 0, 3, 3), (0..4).map { sections.stickyHeaderAtOrBefore(it) })
    }
}
