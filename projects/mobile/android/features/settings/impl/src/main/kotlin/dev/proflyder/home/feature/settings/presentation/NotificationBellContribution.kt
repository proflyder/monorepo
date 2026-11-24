package dev.proflyder.home.feature.settings.presentation

import android.content.res.ColorStateList
import android.view.LayoutInflater
import android.view.View
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.isVisible
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.access.AppAccessSource
import dev.proflyder.home.core.access.allows
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.navigation.Navigator
import dev.proflyder.home.core.navigation.ToolbarContribution
import dev.proflyder.home.core.navigation.ToolbarSlots
import dev.proflyder.home.core.ui.injectedViewModels
import dev.proflyder.home.feature.settings.NotificationHistoryDestination
import dev.proflyder.home.feature.settings.impl.R
import dev.proflyder.home.feature.settings.impl.databinding.SettingsToolbarBellBinding
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch

@ContributesIntoSet(AppScope::class, binding = binding<ToolbarContribution>())
@Inject
class NotificationBellContribution(
    private val access: AppAccessSource,
) : ToolbarContribution {
    override val order: Int = 20

    override val module: AppModule = AppModule.NOTIFICATIONS

    override fun attach(activity: AppCompatActivity, slots: ToolbarSlots, navigator: Navigator) {
        val bell = SettingsToolbarBellBinding.inflate(LayoutInflater.from(activity), slots.end, false)
        slots.end.addView(bell.root)
        bell.settingsBellButton.setOnClickListener { navigator.navigate(NotificationHistoryDestination) }
        val model by activity.injectedViewModels<NotificationBellViewModel>()
        activity.lifecycleScope.launch {
            activity.repeatOnLifecycle(Lifecycle.State.STARTED) {
                launch { model.state.collect { NotificationBell.render(bell, it) } }
                combine(navigator.chrome, access.state) { chrome, state ->
                    !chrome.overlay && state.allows(module)
                }.collect { bell.root.isVisible = it }
            }
        }
    }
}

internal object NotificationBell {
    const val MAX_SHOWN: Int = 99

    fun render(bell: SettingsToolbarBellBinding, unread: Int) {
        val context = bell.root.context
        bell.settingsBellBadge.visibility = if (unread > 0) View.VISIBLE else View.GONE
        bell.settingsBellBadge.text = badgeText(unread, context.getString(R.string.bell_badge_overflow))
        bell.settingsBellButton.contentDescription = when {
            unread > 0 -> context.getString(R.string.bell_description_unread, unread)
            else -> context.getString(R.string.bell_description)
        }
        bell.settingsBellButton.tooltipText = bell.settingsBellButton.contentDescription
        bell.settingsBellButton.iconTint = ColorStateList.valueOf(MaterialColors.getColor(bell.root, tint(unread)))
    }

    private fun tint(unread: Int): Int = when {
        unread > 0 -> androidx.appcompat.R.attr.colorPrimary
        else -> com.google.android.material.R.attr.colorOnSurfaceVariant
    }

    fun badgeText(unread: Int, overflow: String): String = when {
        unread > MAX_SHOWN -> overflow
        else -> unread.toString()
    }
}
