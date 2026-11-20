package dev.proflyder.home.core.designsystem

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class PopupPlacementTest {
    @Test
    fun attachMenuOpensAboveTheAnchorAt640() = assertAbove(SMALL_SCREEN)

    @Test
    fun attachMenuOpensAboveTheAnchorAt800() = assertAbove(LARGE_SCREEN)

    @Test
    fun toolbarMenuOpensDownwards() {
        val place = PopupPlacements.place(STATUS_BAR, STATUS_BAR + TOOLBAR, STATUS_BAR, LARGE_SCREEN - NAV, MENU, 0)
        assertFalse(place.above)
        assertTrue(place.top >= STATUS_BAR + TOOLBAR)
        assertTrue(place.top + place.height <= LARGE_SCREEN - NAV)
    }

    @Test
    fun tooTallMenuScrollsInsideTheLargerSide() {
        val anchorTop = SMALL_SCREEN - NAV - PANEL
        val place = PopupPlacements.place(anchorTop, anchorTop + ANCHOR, STATUS_BAR, SMALL_SCREEN - NAV, SMALL_SCREEN, 0)
        assertTrue(place.above)
        assertTrue(place.top >= STATUS_BAR)
        assertTrue(place.top + place.height <= anchorTop)
    }

    private fun assertAbove(screen: Int) {
        val limitBottom = screen - NAV
        val anchorTop = limitBottom - PANEL
        val place = PopupPlacements.place(anchorTop, anchorTop + ANCHOR, STATUS_BAR, limitBottom, MENU, 0)
        assertTrue("opens upwards", place.above)
        assertTrue("whole menu above the anchor", place.top + place.height <= anchorTop)
        assertTrue("inside the visible area", place.top >= STATUS_BAR)
        assertTrue("not over the bottom navigation", place.top + place.height <= limitBottom)
    }

    private companion object {
        const val SMALL_SCREEN = 640
        const val LARGE_SCREEN = 800
        const val STATUS_BAR = 24
        const val TOOLBAR = 64
        const val NAV = 80
        const val PANEL = 64
        const val ANCHOR = 48
        const val MENU = 128
    }
}
