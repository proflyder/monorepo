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
    val legacyKeys: Set<String> = emptySet(),
) {
    EMERALD(
        key = "emerald",
        titleRes = R.string.theme_emerald_title,
        summaryRes = R.string.theme_emerald_summary,
        overlayRes = R.style.ThemeOverlay_Home_Emerald,
        palette = Palettes.emerald,
        legacyKeys = setOf("current"),
    ),
    SYSTEM(
        key = "system",
        titleRes = R.string.theme_system_title,
        summaryRes = R.string.theme_system_summary,
        overlayRes = R.style.ThemeOverlay_Home_Emerald,
        palette = Palettes.emerald,
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
        val DEFAULT: AppTheme = EMERALD

        fun fromKey(key: String?): AppTheme = entries.firstOrNull { it.key == key || key in it.legacyKeys } ?: DEFAULT
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
