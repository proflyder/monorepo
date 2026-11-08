package dev.proflyder.home.feature.scenarios.domain

import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioDraft
import dev.proflyder.home.core.network.ControlCatalogClient
import dev.proflyder.home.feature.scenarios.ScenarioRepository
import dev.proflyder.home.feature.scenarios.ScenarioValidation
import dev.zacsweers.metro.Inject

@Inject class LoadScenarios(
    private val repository: ScenarioRepository,
) {
    suspend operator fun invoke(): Outcome<List<Scenario>> = repository.list()
}

@Inject class LoadScenario(
    private val repository: ScenarioRepository,
) {
    suspend operator fun invoke(id: String): Outcome<Scenario> = repository.get(id)
}

@Inject class SaveScenario(
    private val repository: ScenarioRepository,
) {
    suspend operator fun invoke(
        existing: Scenario?,
        draft: ScenarioDraft,
        idempotencyKey: String,
    ): Outcome<Scenario> {
        ScenarioValidation.validate(draft)?.let { return Outcome.Failure(it) }
        return if (existing == null) repository.create(draft, idempotencyKey) else repository.update(existing, draft)
    }
}

@Inject class SetScenarioEnabled(
    private val repository: ScenarioRepository,
) {
    suspend operator fun invoke(id: String, enabled: Boolean): Outcome<Scenario> =
        repository.setEnabled(id, enabled)
}

@Inject class DeleteScenario(
    private val repository: ScenarioRepository,
) {
    suspend operator fun invoke(id: String): Outcome<Boolean> = repository.delete(id)
}

@Inject class RunScenario(
    private val repository: ScenarioRepository,
) {
    suspend operator fun invoke(id: String, idempotencyKey: String): Outcome<String> =
        repository.run(id, idempotencyKey)
}

/** Reads the catalog used to pick step entries and condition entities. */
@Inject class LoadScenarioCatalog(
    private val catalogClient: ControlCatalogClient,
) {
    suspend operator fun invoke(): Outcome<ControlCatalog> = catalogClient.fetch()
}
