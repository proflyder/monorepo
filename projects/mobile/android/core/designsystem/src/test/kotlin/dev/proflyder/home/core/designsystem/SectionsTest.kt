package dev.proflyder.home.core.designsystem

import dev.proflyder.home.core.designsystem.list.SectionedAdapter
import dev.proflyder.home.core.designsystem.list.hasRowBelow
import dev.proflyder.home.core.designsystem.list.stickyHeaderAtOrBefore
import org.junit.Assert.assertEquals
import org.junit.Test

class SectionsTest {
    private val headers = setOf(0, 3)
    private val sections = object : SectionedAdapter {
        override fun isSectionHeader(position: Int): Boolean = position in headers
    }

    @Test
    fun onlyRowsFollowedByARowOfTheSameSectionGetADivider() {
        assertEquals(listOf(false, true, false, false, true, false), (0 until 6).map { sections.hasRowBelow(it, 6) })
    }

    @Test
    fun theNearestHeaderAboveARowOwnsIt() {
        assertEquals(listOf(0, 0, 0, 3, 3), (0..4).map { sections.stickyHeaderAtOrBefore(it) })
    }
}
