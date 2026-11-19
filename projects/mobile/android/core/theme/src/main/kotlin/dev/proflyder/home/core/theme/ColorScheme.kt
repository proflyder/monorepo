package dev.proflyder.home.core.theme

import androidx.annotation.ColorInt

data class ColorScheme(
    @ColorInt val primary: Int,
    @ColorInt val onPrimary: Int,
    @ColorInt val primaryContainer: Int,
    @ColorInt val onPrimaryContainer: Int,
    @ColorInt val secondary: Int,
    @ColorInt val onSecondary: Int,
    @ColorInt val secondaryContainer: Int,
    @ColorInt val onSecondaryContainer: Int,
    @ColorInt val tertiary: Int,
    @ColorInt val onTertiary: Int,
    @ColorInt val tertiaryContainer: Int,
    @ColorInt val onTertiaryContainer: Int,
    @ColorInt val error: Int,
    @ColorInt val onError: Int,
    @ColorInt val errorContainer: Int,
    @ColorInt val onErrorContainer: Int,
    @ColorInt val surface: Int,
    @ColorInt val onSurface: Int,
    @ColorInt val onSurfaceVariant: Int,
    @ColorInt val surfaceContainerLow: Int,
    @ColorInt val surfaceContainer: Int,
    @ColorInt val surfaceContainerHigh: Int,
    @ColorInt val surfaceContainerHighest: Int,
    @ColorInt val outline: Int,
    @ColorInt val outlineVariant: Int,
) {
    @ColorInt
    operator fun get(role: ColorRole): Int = when (role) {
        ColorRole.PRIMARY -> primary
        ColorRole.ON_PRIMARY -> onPrimary
        ColorRole.PRIMARY_CONTAINER -> primaryContainer
        ColorRole.ON_PRIMARY_CONTAINER -> onPrimaryContainer
        ColorRole.SECONDARY -> secondary
        ColorRole.ON_SECONDARY -> onSecondary
        ColorRole.SECONDARY_CONTAINER -> secondaryContainer
        ColorRole.ON_SECONDARY_CONTAINER -> onSecondaryContainer
        ColorRole.TERTIARY -> tertiary
        ColorRole.ON_TERTIARY -> onTertiary
        ColorRole.TERTIARY_CONTAINER -> tertiaryContainer
        ColorRole.ON_TERTIARY_CONTAINER -> onTertiaryContainer
        ColorRole.ERROR -> error
        ColorRole.ON_ERROR -> onError
        ColorRole.ERROR_CONTAINER -> errorContainer
        ColorRole.ON_ERROR_CONTAINER -> onErrorContainer
        ColorRole.SURFACE -> surface
        ColorRole.ON_SURFACE -> onSurface
        ColorRole.ON_SURFACE_VARIANT -> onSurfaceVariant
        ColorRole.SURFACE_CONTAINER_LOW -> surfaceContainerLow
        ColorRole.SURFACE_CONTAINER -> surfaceContainer
        ColorRole.SURFACE_CONTAINER_HIGH -> surfaceContainerHigh
        ColorRole.SURFACE_CONTAINER_HIGHEST -> surfaceContainerHighest
        ColorRole.OUTLINE -> outline
        ColorRole.OUTLINE_VARIANT -> outlineVariant
    }
}

data class ThemePalette(val light: ColorScheme, val dark: ColorScheme)

@ColorInt
internal fun rgb(hex: Int): Int = hex or OPAQUE

private const val OPAQUE = -0x1000000
