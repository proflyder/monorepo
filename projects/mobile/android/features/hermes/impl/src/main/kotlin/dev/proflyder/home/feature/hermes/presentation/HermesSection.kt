package dev.proflyder.home.feature.hermes.presentation

import androidx.annotation.StringRes
import dev.proflyder.home.feature.hermes.HermesPanel
import dev.proflyder.home.feature.hermes.impl.R

internal enum class HermesSection(
    val route: String,
    val panel: HermesPanel,
    @param:StringRes val titleRes: Int,
    @param:StringRes val subtitleRes: Int,
) {
    AGENTS("agents", HermesPanel.AGENTS, R.string.hermes_agents_title, R.string.hermes_agents_subtitle),
    REPORTS("reports", HermesPanel.REPORTS, R.string.hermes_reports_title, R.string.hermes_reports_subtitle),
    VOICE("voice", HermesPanel.VOICE, R.string.hermes_voice_title, R.string.hermes_voice_subtitle),
    ;

    companion object {
        fun fromRouteOrNull(route: String?): HermesSection? = entries.firstOrNull { it.route == route }
    }
}
