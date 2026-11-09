package dev.proflyder.home.core.network

import dev.proflyder.home.core.testing.TestFixtures
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioCondition
import dev.proflyder.home.core.model.ScenarioStep
import dev.proflyder.home.core.model.ScenarioTrigger
import dev.proflyder.home.core.model.TimeTrigger
import dev.proflyder.home.core.model.WallTime
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ScenarioJsonTest {
    private fun sample(): Scenario = Scenario(
        id = "sc-1",
        label = "Evening",
        description = "Dim the lights",
        enabled = true,
        revision = 4,
        createdUnix = 100,
        updatedUnix = 200,
        triggers = listOf(
            ScenarioTrigger.Manual,
            ScenarioTrigger.Time(
                TimeTrigger(
                    timezone = "Asia/Almaty",
                    at = WallTime(19, 30),
                    repeat = RepeatKind.WEEKDAYS,
                ),
            ),
        ),
        conditions = listOf(
            ScenarioCondition.EntityState("light.kitchen", "on"),
            ScenarioCondition.TimeWindow(WallTime(8, 0), WallTime(22, 0), listOf(1, 3)),
        ),
        steps = listOf(
            ScenarioStep.Catalog("light.kitchen", "set", TestFixtures.read("brightness_params.json")),
            ScenarioStep.Speak("Good evening"),
            ScenarioStep.Delay(120),
        ),
        continueOnError = true,
        nextDueUnix = 999,
    )

    @Test
    fun updateRequestUsesProtoJsonOneofShapeAndRoundTrips() {
        val request = ScenarioJson.updateRequest(sample(), expectedRevision = 4)
        val requestJson = WireJson.encodeToJsonElement(UpdateScenarioRequestDto.serializer(), request).jsonObject
        val encoded = requestJson.getValue("scenario")
        val triggers = encoded.jsonObject.getValue("triggers").jsonArray
        assertTrue(triggers[0].jsonObject.containsKey("manual"))
        assertEquals("19", triggers[1].jsonObject.getValue("time").jsonObject.getValue("at").jsonObject["hour"]!!.jsonPrimitive.content)
        assertEquals(
            "REPEAT_KIND_WEEKDAYS",
            triggers[1].jsonObject.getValue("time").jsonObject["repeat"]!!.jsonPrimitive.content,
        )

        val conditions = encoded.jsonObject.getValue("conditions").jsonArray
        assertEquals("light.kitchen", conditions[0].jsonObject.getValue("entityState").jsonObject["entityId"]!!.jsonPrimitive.content)
        assertEquals("8", conditions[1].jsonObject.getValue("timeWindow").jsonObject.getValue("start").jsonObject["hour"]!!.jsonPrimitive.content)

        val steps = encoded.jsonObject.getValue("steps").jsonArray
        assertEquals("light.kitchen", steps[0].jsonObject.getValue("catalog").jsonObject["entryId"]!!.jsonPrimitive.content)
        assertEquals("Good evening", steps[1].jsonObject.getValue("speak").jsonObject["text"]!!.jsonPrimitive.content)
        assertEquals("120", steps[2].jsonObject.getValue("delay").jsonObject["seconds"]!!.jsonPrimitive.content)

        val parsed = ScenarioJson.parseScenario(encoded)
        // UpdateRequest carries only client-controlled fields; server fields stay on the server.
        assertEquals(sample().copy(createdUnix = 0, updatedUnix = 0, nextDueUnix = 0), parsed)
        assertEquals("4", requestJson["expectedRevision"]!!.jsonPrimitive.content)
    }

    @Test
    fun unknownOneofVariantsArePreservedVerbatim() {
        val unknownTrigger = TestFixtures.read("unknown_trigger.json")
        val unknownCondition = TestFixtures.read("unknown_condition.json")
        val unknownStep = TestFixtures.read("unknown_step.json")
        val scenario = sample().copy(
            triggers = listOf(ScenarioTrigger.Unknown(unknownTrigger)),
            conditions = listOf(ScenarioCondition.Unknown(unknownCondition)),
            steps = listOf(ScenarioStep.Unknown(unknownStep)),
        )

        val encoded = WireJson.encodeToJsonElement(
            UpdateScenarioRequestDto.serializer(),
            ScenarioJson.updateRequest(scenario, 1),
        ).jsonObject.getValue("scenario")
        val parsed = ScenarioJson.parseScenario(encoded)

        assertTrue(parsed.hasUnsupportedParts)
        assertEquals(
            unknownTrigger,
            (parsed.triggers.single() as ScenarioTrigger.Unknown).rawJson,
        )
        assertEquals(
            Json.parseToJsonElement(unknownCondition),
            Json.parseToJsonElement((parsed.conditions.single() as ScenarioCondition.Unknown).rawJson),
        )
        assertEquals(
            Json.parseToJsonElement(unknownStep),
            Json.parseToJsonElement((parsed.steps.single() as ScenarioStep.Unknown).rawJson),
        )

        // Saving the same scenario again must not drop the retained objects.
        val again = WireJson.encodeToJsonElement(
            UpdateScenarioRequestDto.serializer(),
            ScenarioJson.updateRequest(parsed, 1),
        ).jsonObject.getValue("scenario")
        assertEquals(
            Json.parseToJsonElement(unknownTrigger),
            again.jsonObject.getValue("triggers").jsonArray.single().jsonObject,
        )
    }

    @Test
    fun createRequestCarriesIdempotencyKeyAndDefaults() {
        val request = WireJson.encodeToJsonElement(
            CreateScenarioRequestDto.serializer(),
            ScenarioJson.createRequest(
                dev.proflyder.home.core.model.ScenarioDraft(
                    label = "Morning",
                    idempotencyKey = "key-1",
                ),
            ),
        ).jsonObject
        assertEquals("key-1", request["idempotencyKey"]!!.jsonPrimitive.content)
        assertTrue(request.getValue("triggers").jsonArray.isEmpty())
        assertTrue(request.getValue("steps").jsonArray.isEmpty())
    }

    @Test
    fun parseScenarioWithMissingListsIsEmpty() {
        val parsed = ScenarioJson.parseScenario(
            Json.parseToJsonElement(TestFixtures.read("scenario_minimal.json")) as JsonObject,
        )
        assertTrue(parsed.triggers.isEmpty())
        assertTrue(parsed.conditions.isEmpty())
        assertTrue(parsed.steps.isEmpty())
    }
}
