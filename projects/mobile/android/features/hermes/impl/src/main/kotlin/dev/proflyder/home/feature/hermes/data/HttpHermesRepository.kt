package dev.proflyder.home.feature.hermes.data

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.network.HermesPanelClient
import dev.proflyder.home.feature.hermes.HermesPanel
import dev.proflyder.home.feature.hermes.HermesRepository
import dev.proflyder.home.feature.hermes.HermesState
import dev.proflyder.home.feature.hermes.Loaded
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.util.EnumMap
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class HttpHermesRepository(
    private val client: HermesPanelClient,
    private val scope: CoroutineScope,
) : HermesRepository {
    private val current = MutableStateFlow(HermesState())
    private val jobs = EnumMap<HermesPanel, Job>(HermesPanel::class.java)

    override val state: StateFlow<HermesState> = current.asStateFlow()

    override fun refresh(panel: HermesPanel) {
        jobs[panel]?.cancel()
        jobs[panel] = when (panel) {
            HermesPanel.AGENTS -> load(HermesState::agents, { copy(agents = it) }, client::agents)
            HermesPanel.REPORTS -> load(HermesState::reports, { copy(reports = it) }, client::reports)
            HermesPanel.VOICE -> load(HermesState::voice, { copy(voice = it) }, client::voice)
            HermesPanel.TOOLSETS -> load(HermesState::toolsets, { copy(toolsets = it) }, client::toolsets)
            HermesPanel.SKILLS -> load(HermesState::skills, { copy(skills = it) }, client::skills)
        }
    }

    private fun <T : Any> load(
        select: (HermesState) -> Loaded<T>,
        replace: HermesState.(Loaded<T>) -> HermesState,
        fetch: suspend () -> Outcome<T>,
    ): Job = scope.launch {
        current.update { it.replace(select(it).copy(loading = true)) }
        val result = fetch()
        current.update { it.replace(select(it).applied(result)) }
    }
}

internal fun <T : Any> Loaded<T>.applied(result: Outcome<T>): Loaded<T> = when (result) {
    is Outcome.Success -> Loaded(loading = false, value = result.value, failure = null)
    is Outcome.Failure -> copy(loading = false, failure = result.failure)
}
