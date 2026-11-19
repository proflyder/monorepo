package dev.proflyder.home.core.theme

import androidx.annotation.AttrRes
import com.google.android.material.R as MaterialR

/**
 * The Material 3 colour roles every app theme defines. Screens never name a
 * colour: they read one of these roles through its theme attribute
 * (`?attr/colorPrimary`, `MaterialColors.getColor(view, attr)`).
 */
enum class ColorRole(@AttrRes val attr: Int) {
    PRIMARY(androidx.appcompat.R.attr.colorPrimary),
    ON_PRIMARY(MaterialR.attr.colorOnPrimary),
    PRIMARY_CONTAINER(MaterialR.attr.colorPrimaryContainer),
    ON_PRIMARY_CONTAINER(MaterialR.attr.colorOnPrimaryContainer),
    SECONDARY(MaterialR.attr.colorSecondary),
    ON_SECONDARY(MaterialR.attr.colorOnSecondary),
    SECONDARY_CONTAINER(MaterialR.attr.colorSecondaryContainer),
    ON_SECONDARY_CONTAINER(MaterialR.attr.colorOnSecondaryContainer),
    TERTIARY(MaterialR.attr.colorTertiary),
    ON_TERTIARY(MaterialR.attr.colorOnTertiary),
    TERTIARY_CONTAINER(MaterialR.attr.colorTertiaryContainer),
    ON_TERTIARY_CONTAINER(MaterialR.attr.colorOnTertiaryContainer),
    ERROR(androidx.appcompat.R.attr.colorError),
    ON_ERROR(MaterialR.attr.colorOnError),
    ERROR_CONTAINER(MaterialR.attr.colorErrorContainer),
    ON_ERROR_CONTAINER(MaterialR.attr.colorOnErrorContainer),
    SURFACE(MaterialR.attr.colorSurface),
    ON_SURFACE(MaterialR.attr.colorOnSurface),
    ON_SURFACE_VARIANT(MaterialR.attr.colorOnSurfaceVariant),
    SURFACE_CONTAINER_LOW(MaterialR.attr.colorSurfaceContainerLow),
    SURFACE_CONTAINER(MaterialR.attr.colorSurfaceContainer),
    SURFACE_CONTAINER_HIGH(MaterialR.attr.colorSurfaceContainerHigh),
    SURFACE_CONTAINER_HIGHEST(MaterialR.attr.colorSurfaceContainerHighest),
    OUTLINE(MaterialR.attr.colorOutline),
    OUTLINE_VARIANT(MaterialR.attr.colorOutlineVariant),
}
