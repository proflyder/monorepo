package dev.proflyder.home.core.network

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
    private val json = Json { ignoreUnknownKeys = true }

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
            ScenarioStep.Catalog("light.kitchen", "set", """{"brightness":40}"""),
            ScenarioStep.Speak("Good evening"),
            ScenarioStep.Delay(120),
        ),
        continueOnError = true,
        nextDueUnix = 999,
    )

    @Test
    fun updateRequestUsesProtoJsonOneofShapeAndRoundTrips() {
        val request = ScenarioJson.updateRequest(sample(), expectedRevision = 4)
        val encoded = request.jsonObject.getValue("scenario")
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
        assertEquals("4", request.jsonObject["expectedRevision"]!!.jsonPrimitive.content)
    }

    @Test
    fun unknownOneofVariantsArePreservedVerbatim() {
        val unknownTrigger = """{"newTrigger":{"kind":"future","value":7}}"""
        val unknownCondition = """{"weatherState":{"entityId":"weather.home","state":"sunny"}}"""
        val unknownStep = """{"waitUntil":{"timestamp":"2099-01-01T00:00:00Z"}}"""
        val scenario = sample().copy(
            triggers = listOf(ScenarioTrigger.Unknown(unknownTrigger)),
            conditions = listOf(ScenarioCondition.Unknown(unknownCondition)),
            steps = listOf(ScenarioStep.Unknown(unknownStep)),
        )

        val encoded = ScenarioJson.updateRequest(scenario, 1).jsonObject.getValue("scenario")
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
        val again = ScenarioJson.updateRequest(parsed, 1).jsonObject.getValue("scenario")
        assertEquals(
            Json.parseToJsonElement(unknownTrigger),
            again.jsonObject.getValue("triggers").jsonArray.single().jsonObject,
        )
    }

    @Test
    fun createRequestCarriesIdempotencyKeyAndDefaults() {
        val request = ScenarioJson.createRequest(
            dev.proflyder.home.core.model.ScenarioDraft(
                label = "Morning",
                idempotencyKey = "key-1",
            ),
        )
        assertEquals("key-1", request["idempotencyKey"]!!.jsonPrimitive.content)
        assertTrue(request.getValue("triggers").jsonArray.isEmpty())
        assertTrue(request.getValue("steps").jsonArray.isEmpty())
    }

    @Test
    fun parseScenarioWithMissingListsIsEmpty() {
        val parsed = ScenarioJson.parseScenario(
            Json.parseToJsonElement("""{"id":"s","label":"L","revision":"1"}""") as JsonObject,
        )
        assertTrue(parsed.triggers.isEmpty())
        assertTrue(parsed.conditions.isEmpty())
        assertTrue(parsed.steps.isEmpty())
    }
}
