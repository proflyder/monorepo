package dev.proflyder.home.feature.scenarios

import dev.proflyder.home.core.testing.TestFixtures
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioDraft
import dev.proflyder.home.core.model.ScenarioStep
import dev.proflyder.home.core.model.ScenarioTrigger
import dev.proflyder.home.core.model.TimeTrigger
import dev.proflyder.home.core.model.WallTime
import dev.proflyder.home.feature.scenarios.domain.DeleteScenario
import dev.proflyder.home.feature.scenarios.domain.RunScenario
import dev.proflyder.home.feature.scenarios.domain.SaveScenario
import dev.proflyder.home.feature.scenarios.domain.SetScenarioEnabled
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ScenarioUseCasesTest {
    private class FakeRepository : ScenarioRepository {
        var created: Pair<ScenarioDraft, String>? = null
        var updated: Pair<Scenario, ScenarioDraft>? = null
        var runKey: String? = null
        var enabled: Pair<String, Boolean>? = null
        var deleted: String? = null
        var next: Outcome<Scenario> = Outcome.Success(scenario())

        override suspend fun list(): Outcome<List<Scenario>> = Outcome.Success(listOf(scenario()))
        override suspend fun get(id: String): Outcome<Scenario> = next
        override suspend fun create(draft: ScenarioDraft, idempotencyKey: String): Outcome<Scenario> {
            created = draft to idempotencyKey
            return next
        }
        override suspend fun update(existing: Scenario, draft: ScenarioDraft): Outcome<Scenario> {
            updated = existing to draft
            return next
        }
        override suspend fun setEnabled(id: String, enabled: Boolean): Outcome<Scenario> {
            this.enabled = id to enabled
            return next
        }
        override suspend fun delete(id: String): Outcome<Boolean> {
            deleted = id
            return Outcome.Success(true)
        }
        override suspend fun run(id: String, idempotencyKey: String): Outcome<String> {
            runKey = idempotencyKey
            return Outcome.Success("evt-1")
        }
    }

    private fun draft() = ScenarioDraft(
        label = "Evening",
        triggers = listOf(ScenarioTrigger.Time(TimeTrigger("UTC", WallTime(19, 0), RepeatKind.DAILY))),
        steps = listOf(ScenarioStep.Speak("Good evening")),
    )

    @Test
    fun createUsesIdempotencyKeyAndValidatedDraft() = runBlocking {
        val repository = FakeRepository()
        val result = SaveScenario(repository)(existing = null, draft = draft(), idempotencyKey = "key-1")
        assertTrue(result is Outcome.Success)
        assertEquals("key-1", repository.created!!.second)
        assertEquals("Evening", repository.created!!.first.label)
    }

    @Test
    fun updateReusesTheExistingRevision() = runBlocking {
        val repository = FakeRepository()
        val existing = scenario().copy(revision = 6)
        SaveScenario(repository)(existing, draft().copy(label = "New"), idempotencyKey = "")
        assertEquals(existing, repository.updated!!.first)
        assertEquals("New", repository.updated!!.second.label)
    }

    @Test
    fun invalidDraftNeverReachesTheRepository() = runBlocking {
        val repository = FakeRepository()
        val result = SaveScenario(repository)(
            existing = null,
            draft = draft().copy(label = "a".repeat(81)),
            idempotencyKey = "key",
        )
        assertTrue(result is Outcome.Failure)
        assertNull(repository.created)
    }

    @Test
    fun unknownPartsNeverReachTheRepository() = runBlocking {
        val repository = FakeRepository()
        val result = SaveScenario(repository)(
            existing = scenario(),
            draft = draft().copy(triggers = listOf(ScenarioTrigger.Unknown(TestFixtures.read("unknown_empty.json")))),
            idempotencyKey = "",
        )
        assertTrue(result is Outcome.Failure)
        assertNull(repository.updated)
    }

    @Test
    fun simpleDelegationsPreserveTheirArguments() = runBlocking {
        val repository = FakeRepository()
        SetScenarioEnabled(repository)("sc-1", false)
        assertEquals("sc-1" to false, repository.enabled)
        DeleteScenario(repository)("sc-1")
        assertEquals("sc-1", repository.deleted)
        val run = RunScenario(repository)("sc-1", "run-key")
        assertEquals("evt-1", (run as Outcome.Success).value)
        assertEquals("run-key", repository.runKey)
    }

    private companion object {
        fun scenario() = Scenario(
            id = "sc-1",
            label = "Evening",
            description = "",
            enabled = true,
            revision = 3,
            createdUnix = 0,
            updatedUnix = 0,
            triggers = emptyList(),
            conditions = emptyList(),
            steps = emptyList(),
            continueOnError = false,
            nextDueUnix = 0,
        )
    }
}
