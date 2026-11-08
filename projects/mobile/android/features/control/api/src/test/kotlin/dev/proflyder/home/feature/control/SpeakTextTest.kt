package dev.proflyder.home.feature.control

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SpeakTextTest {
    @Test
    fun trimsAndCollapsesWhitespace() {
        assertEquals("Привет, дом.", SpeakText.normalize("  Привет,\n дом.  "))
        assertEquals("a b c", SpeakText.normalize("a\t b\r\nc"))
    }

    @Test
    fun rejectsEmptyAndOverlongText() {
        assertNull(SpeakText.normalize(""))
        assertNull(SpeakText.normalize("  \n\t "))
        assertEquals("ж".repeat(SpeakText.MAX_LENGTH), SpeakText.normalize("ж".repeat(SpeakText.MAX_LENGTH)))
        assertNull(SpeakText.normalize("ж".repeat(SpeakText.MAX_LENGTH + 1)))
    }

    @Test
    fun rejectsControlCharactersThatSurviveWhitespaceCollapsing() {
        assertNull(SpeakText.normalize("привет\u0000мир"))
    }

    @Test
    fun remainingNeverGoesNegative() {
        assertEquals(500, SpeakText.remaining(""))
        assertEquals(494, SpeakText.remaining("привет"))
        assertEquals(0, SpeakText.remaining("ж".repeat(600)))
    }
}
