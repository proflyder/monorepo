package dev.proflyder.home.core.launchericon

import dev.proflyder.home.core.theme.AppTheme

interface LauncherIcons {
    fun shown(): AppTheme

    fun show(theme: AppTheme)
}
