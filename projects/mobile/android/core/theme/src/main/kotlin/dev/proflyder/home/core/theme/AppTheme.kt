package dev.proflyder.home.core.theme

import androidx.annotation.StringRes
import androidx.annotation.StyleRes

/**
 * The theme registry. Adding a theme is one entry here plus its
 * `ThemeOverlay.Home.*` style and `@color/theme_*` resources; screens only read
 * theme attributes, so none of them changes.
 *
 * Every entry has a full static [palette]. A [dynamic] theme replaces it with
 * the wallpaper colours (Material You) where the system supports them and
 * shows the static palette everywhere else.
 */
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

/** Light/dark choice, independent of the palette. */
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

/** What the owner picked on the Appearance screen. */
data class Appearance(
    val theme: AppTheme = AppTheme.DEFAULT,
    val mode: ThemeMode = ThemeMode.DEFAULT,
)
