package dev.proflyder.home.feature.chat.presentation

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SwipeReplyMathTest {
    private val math = SwipeReplyMath(threshold = 72f, maxOffset = 108f)

    @Test
    fun onlyLeftwardsDragMovesTheBubbleAndItIsCapped() {
        assertEquals(0f, math.offset(40f), 0f)
        assertEquals(-50f, math.offset(-50f), 0f)
        assertEquals(-108f, math.offset(-400f), 0f)
    }

    @Test
    fun iconProgressGrowsUntilTheThreshold() {
        assertEquals(0f, math.progress(0f), 0f)
        assertEquals(0.5f, math.progress(-36f), 0.001f)
        assertEquals(1f, math.progress(-100f), 0f)
    }

    @Test
    fun replyArmsOnlyPastTheThreshold() {
        assertFalse(math.crossed(-71f))
        assertTrue(math.crossed(-72f))
    }

    @Test
    fun swipeStartsOnlyForHorizontalLeftwardsMovesBeyondSlop() {
        assertTrue(math.startsSwipe(dx = -20f, dy = 5f, slop = 8))
        assertFalse(math.startsSwipe(dx = 20f, dy = 0f, slop = 8))
        assertFalse(math.startsSwipe(dx = -6f, dy = 0f, slop = 8))
        assertFalse(math.startsSwipe(dx = -20f, dy = 30f, slop = 8))
    }
}
