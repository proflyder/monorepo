package dev.proflyder.home.core.theme

import android.content.Context
import androidx.annotation.ColorInt
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject

interface NotificationAccent {
    @ColorInt
    fun color(): Int
}

@ContributesBinding(AppScope::class)
@Inject
class ThemedNotificationAccent(
    private val context: Context,
    private val appearance: AppearanceController,
) : NotificationAccent {
    override fun color(): Int = AppThemes.color(context, appearance.state.value.theme, ColorRole.PRIMARY)
}
