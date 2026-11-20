package dev.proflyder.home.core.designsystem

import android.content.res.ColorStateList
import androidx.core.graphics.ColorUtils
import com.google.android.material.button.MaterialButton
import com.google.android.material.color.MaterialColors
import com.google.android.material.R as MaterialR

enum class Emphasis(val container: Int, val content: Int) {
    FILLED(MaterialR.attr.colorPrimary, MaterialR.attr.colorOnPrimary),
    TONAL(MaterialR.attr.colorSecondaryContainer, MaterialR.attr.colorOnSecondaryContainer),
}

fun MaterialButton.setEmphasis(emphasis: Emphasis) {
    val onSurface = MaterialColors.getColor(this, MaterialR.attr.colorOnSurface)
    backgroundTintList = states(MaterialColors.getColor(this, emphasis.container), ColorUtils.setAlphaComponent(onSurface, DISABLED_CONTAINER))
    setTextColor(states(MaterialColors.getColor(this, emphasis.content), ColorUtils.setAlphaComponent(onSurface, DISABLED_CONTENT)))
    iconTint = textColors
}

private fun states(enabled: Int, disabled: Int): ColorStateList = ColorStateList(
    arrayOf(intArrayOf(-android.R.attr.state_enabled), intArrayOf()),
    intArrayOf(disabled, enabled),
)

private const val DISABLED_CONTAINER = 31
private const val DISABLED_CONTENT = 97
