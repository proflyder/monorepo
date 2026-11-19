package dev.proflyder.home.core.theme

import androidx.annotation.StringRes
import androidx.annotation.StyleRes

enum class AppTheme(
    val key: String,
    @StringRes val titleRes: Int,
    @StringRes val summaryRes: Int,
    @StyleRes val overlayRes: Int,
    val palette: ThemePalette,
    val dynamic: Boolean = false,
) {
    CURRENT(
        key = "current",
        titleRes = R.string.theme_current_title,
        summaryRes = R.string.theme_current_summary,
        overlayRes = R.style.ThemeOverlay_Home_Current,
        palette = Palettes.current,
    ),
    SYSTEM(
        key = "system",
        titleRes = R.string.theme_system_title,
        summaryRes = R.string.theme_system_summary,
        overlayRes = R.style.ThemeOverlay_Home_Current,
        palette = Palettes.current,
        dynamic = true,
    ),
    NEON(
        key = "neon",
        titleRes = R.string.theme_neon_title,
        summaryRes = R.string.theme_neon_summary,
        overlayRes = R.style.ThemeOverlay_Home_Neon,
        palette = Palettes.neon,
    ),
    ;

    companion object {
        val DEFAULT: AppTheme = CURRENT

        fun fromKey(key: String?): AppTheme = entries.firstOrNull { it.key == key } ?: DEFAULT
    }
}

enum class ThemeMode(val key: String) {
    SYSTEM("system"),
    LIGHT("light"),
    DARK("dark"),
    ;

    companion object {
        val DEFAULT: ThemeMode = SYSTEM

        fun fromKey(key: String?): ThemeMode = entries.firstOrNull { it.key == key } ?: DEFAULT
    }
}

data class Appearance(
    val theme: AppTheme = AppTheme.DEFAULT,
    val mode: ThemeMode = ThemeMode.DEFAULT,
)
