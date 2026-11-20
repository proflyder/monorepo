package dev.proflyder.home.core.theme

import android.content.Context
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ThemeNamesTest {
    @Test
    @Config(qualifiers = "ru")
    fun russianNamesAreUniqueAndOwnTheirTheme() {
        val names = titles()
        assertEquals(listOf("Изумруд", "Системная", "Неон"), names)
        assertFalse(names.any { it.contains("Текущ") })
    }

    @Test
    @Config(qualifiers = "en")
    fun englishNamesAreUnique() {
        assertEquals(listOf("Emerald", "System", "Neon"), titles())
    }

    private fun titles(): List<String> {
        val context: Context = RuntimeEnvironment.getApplication()
        val names = AppTheme.entries.map { context.getString(it.titleRes) }
        assertEquals(names.size, names.toSet().size)
        return names
    }
}
