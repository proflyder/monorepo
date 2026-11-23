package dev.proflyder.home.core.launchericon

import dev.proflyder.home.core.theme.AppTheme

object LauncherAliases {
    const val PACKAGE: String = "dev.proflyder.home.launcher"

    val manifestDefault: AppTheme = AppTheme.DEFAULT

    fun className(theme: AppTheme): String = "$PACKAGE.${aliasName(theme)}"

    private fun aliasName(theme: AppTheme): String = when (theme) {
        AppTheme.EMERALD -> "Emerald"
        AppTheme.SYSTEM -> "System"
        AppTheme.NEON -> "Neon"
    }
}
