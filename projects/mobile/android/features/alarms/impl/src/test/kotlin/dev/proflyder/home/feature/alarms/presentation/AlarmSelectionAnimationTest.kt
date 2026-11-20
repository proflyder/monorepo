package dev.proflyder.home.feature.alarms.presentation

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AlarmSelectionAnimationTest {
    @Test
    fun sameAlarmSelectionChangeAnimates() {
        assertTrue(shouldAnimateSelection("a", false, "a", true))
        assertTrue(shouldAnimateSelection("a", true, "a", false))
    }

    @Test
    fun firstBindDoesNotAnimate() {
        assertFalse(shouldAnimateSelection(null, null, "a", true))
    }

    @Test
    fun unchangedSelectionDoesNotAnimate() {
        assertFalse(shouldAnimateSelection("a", true, "a", true))
    }

    @Test
    fun recycledHolderForAnotherAlarmDoesNotAnimate() {
        assertFalse(shouldAnimateSelection("a", true, "b", true))
        assertFalse(shouldAnimateSelection("a", true, "b", false))
    }
}
