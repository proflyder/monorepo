package dev.proflyder.home.feature.scenarios

import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.ScenarioCondition
import dev.proflyder.home.core.model.ScenarioStep
import dev.proflyder.home.core.model.ScenarioTrigger
import dev.proflyder.home.core.model.TimeTrigger
import dev.proflyder.home.core.model.WallTime
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ScenarioEditorDraftCodecTest {
    private fun draft() = ScenarioEditorDraft(
        existingId = "sc-1",
        existingRevision = 7,
        label = "Evening",
        description = "Dim the lights",
        enabled = false,
        continueOnError = true,
        triggers = listOf(
            ScenarioTrigger.Manual,
            ScenarioTrigger.Time(
                TimeTrigger("Asia/Almaty", WallTime(19, 30), RepeatKind.CUSTOM, weekdays = listOf(1, 3, 5)),
            ),
            ScenarioTrigger.Unknown("""{"futureTrigger":{"x":1}}"""),
        ),
        conditions = listOf(
            ScenarioCondition.EntityState("light.kitchen", "on"),
            ScenarioCondition.TimeWindow(WallTime(8, 0), WallTime(22, 0), listOf(6, 7)),
            ScenarioCondition.Unknown("""{"futureCondition":{}}"""),
        ),
        steps = listOf(
            ScenarioStep.Catalog("light.kitchen", "set", """{"brightness":40}"""),
            ScenarioStep.Speak("Good evening"),
            ScenarioStep.Delay(90),
            ScenarioStep.Unknown("""{"futureStep":{}}"""),
        ),
    )

    @Test
    fun roundTripsEveryOneofAndUnknownPart() {
        val restored = ScenarioEditorDraftCodec.fromMap(ScenarioEditorDraftCodec.toMap(draft()))
        assertEquals(draft(), restored)
    }

    @Test
    fun fromScenarioKeepsRevisionAndUnsupportedParts() {
        val draft = ScenarioEditorDraftCodec.fromScenario(
            dev.proflyder.home.core.model.Scenario(
                id = "sc-1",
                label = "L",
                description = "",
                enabled = true,
                revision = 9,
                createdUnix = 0,
                updatedUnix = 0,
                triggers = listOf(ScenarioTrigger.Unknown("""{"future":{}}""")),
                conditions = emptyList(),
                steps = emptyList(),
                continueOnError = false,
                nextDueUnix = 0,
            ),
        )
        assertEquals(9L, draft.existingRevision)
        assertEquals(true, draft.hasUnsupportedParts)
    }

    @Test
    fun absentOrGarbageDraftIsNull() {
        assertNull(ScenarioEditorDraftCodec.fromMap(emptyMap()))
        assertNull(ScenarioEditorDraftCodec.fromMap(mapOf("scenario_draft" to "{not json")))
    }

    @Test
    fun toScenarioDraftTrimsOnlyFreeText() {
        val payload = draft().copy(label = "  Evening  ", description = "  text  ").toScenarioDraft("key")
        assertEquals("Evening", payload.label)
        assertEquals("text", payload.description)
        assertEquals("key", payload.idempotencyKey)
        assertEquals(draft().steps, payload.steps)
    }
}
