package dev.proflyder.home.core.theme

import android.app.Activity
import android.content.Context
import androidx.annotation.ColorInt
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.color.DynamicColors
import com.google.android.material.color.DynamicColorsOptions
import com.google.android.material.color.MaterialColors
import com.google.android.material.R as MaterialR

object AppThemes {
    private val dynamicOptions: DynamicColorsOptions = DynamicColorsOptions.Builder().build()

    val isDynamicAvailable: Boolean get() = DynamicColors.isDynamicColorAvailable()

    fun applyTo(activity: Activity, theme: AppTheme) {
        activity.theme.applyStyle(theme.overlayRes, true)
        if (theme.dynamic) DynamicColors.applyToActivityIfAvailable(activity, dynamicOptions)
    }

    fun wrap(context: Context, theme: AppTheme): Context {
        val themed = ContextThemeWrapper(context, theme.overlayRes)
        return when {
            theme.dynamic -> DynamicColors.wrapContextIfAvailable(themed)
            else -> themed
        }
    }

    @ColorInt
    fun color(context: Context, theme: AppTheme, role: ColorRole): Int {
        val base = ContextThemeWrapper(context, MaterialR.style.Theme_Material3_DayNight_NoActionBar)
        return MaterialColors.getColor(wrap(base, theme), role.attr, theme.palette.dark[role])
    }
}
