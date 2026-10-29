package dev.proflyder.home.feature.control

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class LightColorsTest {
    private fun r(c: Int) = c shr 16 and 0xFF
    private fun b(c: Int) = c and 0xFF

    @Test
    fun warmWhiteIsRedderThanCool() {
        val warm = LightColors.kelvinToArgb(2700)
        val cool = LightColors.kelvinToArgb(6500)
        assertTrue(r(warm) > b(warm))
        assertTrue(b(cool) > b(warm))
        assertEquals(0xFF, warm ushr 24)
    }

    @Test
    fun primaryHuesAndDesaturation() {
        assertEquals(0xFFFF0000.toInt(), LightColors.hsToArgb(0, 100))
        assertEquals(0xFF00FF00.toInt(), LightColors.hsToArgb(120, 100))
        assertEquals(0xFF0000FF.toInt(), LightColors.hsToArgb(240, 100))
        assertEquals(0xFFFFFFFF.toInt(), LightColors.hsToArgb(200, 0))
        assertEquals(LightColors.hsToArgb(0, 100), LightColors.hsToArgb(360, 100))
    }
}
