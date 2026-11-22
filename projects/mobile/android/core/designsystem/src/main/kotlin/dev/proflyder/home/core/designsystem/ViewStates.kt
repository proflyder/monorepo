package dev.proflyder.home.core.designsystem

import android.view.View

const val DISABLED_ALPHA = 0.5f

private const val CHEVRON_EXPANDED_DEGREES = 180f

fun View.bindAvailability(available: Boolean) {
    alpha = if (available) 1f else DISABLED_ALPHA
}

fun View.bindChevron(expanded: Boolean) {
    rotation = if (expanded) CHEVRON_EXPANDED_DEGREES else 0f
}
