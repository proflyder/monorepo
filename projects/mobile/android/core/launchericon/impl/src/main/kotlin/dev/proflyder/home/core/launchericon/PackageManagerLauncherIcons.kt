package dev.proflyder.home.core.launchericon

import android.content.ComponentName
import android.content.Context
import android.content.pm.PackageManager
import dev.proflyder.home.core.theme.AppTheme
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject

@ContributesBinding(AppScope::class)
@Inject
class PackageManagerLauncherIcons(private val context: Context) : LauncherIcons {
    private val packageManager: PackageManager get() = context.packageManager

    override fun shown(): AppTheme = AppTheme.entries.firstOrNull(::isEnabled) ?: LauncherAliases.manifestDefault

    override fun show(theme: AppTheme) {
        setEnabled(theme, true)
        AppTheme.entries.filter { it != theme }.forEach { setEnabled(it, false) }
    }

    private fun setEnabled(theme: AppTheme, enabled: Boolean) {
        if (isEnabled(theme) == enabled) return
        packageManager.setComponentEnabledSetting(component(theme), stateOf(enabled), PackageManager.DONT_KILL_APP)
    }

    private fun isEnabled(theme: AppTheme): Boolean = when (packageManager.getComponentEnabledSetting(component(theme))) {
        PackageManager.COMPONENT_ENABLED_STATE_ENABLED -> true
        PackageManager.COMPONENT_ENABLED_STATE_DEFAULT -> theme == LauncherAliases.manifestDefault
        else -> false
    }

    private fun stateOf(enabled: Boolean): Int = when {
        enabled -> PackageManager.COMPONENT_ENABLED_STATE_ENABLED
        else -> PackageManager.COMPONENT_ENABLED_STATE_DISABLED
    }

    private fun component(theme: AppTheme): ComponentName =
        ComponentName(context.packageName, LauncherAliases.className(theme))
}
