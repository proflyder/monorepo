package dev.proflyder.home.core.designsystem

import android.content.Context
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.theme.AppTheme
import org.robolectric.RuntimeEnvironment

internal fun themed(theme: AppTheme): Context {
    val base = ContextThemeWrapper(RuntimeEnvironment.getApplication(), R.style.Base_Theme_Home)
    base.theme.applyStyle(theme.overlayRes, true)
    return base
}

internal fun color(context: Context, attr: Int): Int = MaterialColors.getColor(context, attr, 0)

internal val BUTTON_VARIANTS = listOf("Filled", "Tonal", "Outlined", "Text", "Destructive")

internal val SIZES = mapOf("S" to 32, "M" to 40, "L" to 48)
