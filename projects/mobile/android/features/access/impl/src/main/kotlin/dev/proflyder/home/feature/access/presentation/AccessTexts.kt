package dev.proflyder.home.feature.access.presentation

import android.content.Context
import dev.proflyder.home.core.model.AccessToken
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.feature.access.impl.R

internal val OFFERED_MODULES: List<AppModule> =
    AppModule.entries.filter { it != AppModule.BASE && it != AppModule.ADMIN && it != AppModule.SECRETS }

internal fun moduleTitleRes(module: AppModule): Int = when (module) {
    AppModule.BASE -> R.string.access_module_base
    AppModule.HOME -> R.string.access_module_home
    AppModule.SCENARIOS -> R.string.access_module_scenarios
    AppModule.ALARMS_TIMERS -> R.string.access_module_alarms_timers
    AppModule.INTERCOM_CALL -> R.string.access_module_intercom_call
    AppModule.NOTIFICATIONS -> R.string.access_module_notifications
    AppModule.CHAT -> R.string.access_module_chat
    AppModule.VPN -> R.string.access_module_vpn
    AppModule.SYSTEM -> R.string.access_module_system
    AppModule.INFRA_REPORTS -> R.string.access_module_infra_reports
    AppModule.ALERTS -> R.string.access_module_alerts
    AppModule.SECRETS -> R.string.access_module_secrets
    AppModule.ADMIN -> R.string.access_module_admin
}

internal fun moduleSummary(context: Context, modules: Set<AppModule>): String {
    val names = AppModule.entries
        .filter { it != AppModule.BASE && it in modules }
        .map { context.getString(moduleTitleRes(it)) }
    return names.takeIf { it.isNotEmpty() }?.joinToString(MODULE_SEPARATOR)
        ?: context.getString(R.string.access_modules_base_only)
}

internal fun AccessToken.summary(context: Context): String = moduleSummary(context, modules)

private const val MODULE_SEPARATOR = " · "
