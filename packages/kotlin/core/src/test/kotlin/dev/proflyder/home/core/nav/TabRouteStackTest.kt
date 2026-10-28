package dev.proflyder.home.core.nav

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TabRouteStackTest {
    @Test
    fun tabSwitchResetsTargetTabToItsRoot() {
        val stack = TabRouteStack("alarms", "alarms/list")
        stack.push("alarms/editor/new")
        assertEquals("alarms/editor/new", stack.currentRoute())

        stack.selectTab("timers", "timers/list")
        assertEquals("timers/list", stack.currentRoute())
        assertTrue(stack.isAtRoot())

        stack.push("timers/editor/new")
        stack.selectTab("alarms", "alarms/list")
        assertEquals("alarms/list", stack.currentRoute())
        assertTrue(stack.isAtRoot())
    }

    @Test
    fun popNeverRemovesTabRoot() {
        val stack = TabRouteStack("alarms", "alarms/list")
        assertNull(stack.pop())
        assertEquals("alarms/list", stack.currentRoute())

        stack.push("alarms/editor/new")
        assertEquals("alarms/list", stack.pop())
        assertNull(stack.pop())
    }

    @Test
    fun duplicatePushDoesNotGrowStackOrReportAPush() {
        val stack = TabRouteStack("alarms", "alarms/list")
        assertTrue(stack.push("alarms/editor/new"))
        assertFalse(stack.push("alarms/editor/new"))
        assertEquals(2, stack.currentStack().size)
    }

    @Test
    fun resetTabDropsInnerRoutesWhenTabReselected() {
        val stack = TabRouteStack("alarms", "alarms/list")
        stack.push("alarms/editor/new")
        stack.resetTab("alarms", "alarms/list")
        assertEquals("alarms/list", stack.currentRoute())
        assertEquals(listOf("alarms/list"), stack.currentStack())
    }

    @Test
    fun snapshotOnlyDescribesTheSelectedTab() {
        val stack = TabRouteStack("alarms", "alarms/list")
        stack.push("alarms/editor/existing")
        stack.selectTab("timers", "timers/list")
        stack.push("timers/editor/new")

        val snapshot = stack.snapshot()
        assertEquals(setOf("timers"), snapshot.keys)
        assertEquals(listOf("timers/list", "timers/editor/new"), snapshot["timers"])
    }

    @Test
    fun restoreKeepsAValidatedSelectedStack() {
        val stack = TabRouteStack("alarms", "alarms/list")
        stack.restore(
            tabId = "alarms",
            routes = listOf("alarms/list", "alarms/editor/existing"),
            rootRoute = "alarms/list",
        ) { it.startsWith("alarms/") }

        assertEquals("alarms", stack.currentTabId)
        assertEquals("alarms/editor/existing", stack.currentRoute())
        assertEquals(listOf("alarms/list", "alarms/editor/existing"), stack.currentStack())
    }

    @Test
    fun restoreFallsBackToRootForForeignOrMalformedRoutes() {
        val stack = TabRouteStack("alarms", "alarms/list")

        stack.restore(
            tabId = "alarms",
            routes = listOf("timers/list"),
            rootRoute = "alarms/list",
        )
        assertEquals(listOf("alarms/list"), stack.currentStack())

        stack.restore(
            tabId = "alarms",
            routes = listOf("alarms/list", "timers/editor/1"),
            rootRoute = "alarms/list",
        ) { it.startsWith("alarms/") }
        assertEquals(listOf("alarms/list"), stack.currentStack())

        stack.restore(
            tabId = "alarms",
            routes = listOf("alarms/list", " "),
            rootRoute = "alarms/list",
        )
        assertEquals(listOf("alarms/list"), stack.currentStack())

        stack.restore(
            tabId = "alarms",
            routes = emptyList(),
            rootRoute = "alarms/list",
        )
        assertEquals(listOf("alarms/list"), stack.currentStack())
    }
}
