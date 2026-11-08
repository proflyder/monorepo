package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioCondition
import dev.proflyder.home.core.model.ScenarioDraft
import dev.proflyder.home.core.model.ScenarioStep
import dev.proflyder.home.core.model.ScenarioTrigger
import dev.proflyder.home.core.model.TimeTrigger
import dev.proflyder.home.core.model.WallTime
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

/**
 * Explicit protobuf-JSON mapping for home.v1.ScenarioService.
 *
 * The oneof shape is the standard protobuf-JSON one: the active variant is a
 * field of the message object, for example `{"time":{...}}` or
 * `{"speak":{"text":"Hello"}}`. A variant this client does not know is kept as
 * its original JSON object in [ScenarioTrigger.Unknown] and friends, so a newer
 * server definition survives a load/save cycle unchanged.
 */
object ScenarioJson {
    const val SERVICE = "home.v1.ScenarioService"

    fun createRequest(draft: ScenarioDraft): JsonObject = buildJsonObject {
        put("label", draft.label)
        put("description", draft.description)
        put("enabled", draft.enabled)
        put("continueOnError", draft.continueOnError)
        put("triggers", triggers(draft.triggers))
        put("conditions", conditions(draft.conditions))
        put("steps", steps(draft.steps))
        if (draft.idempotencyKey.isNotBlank()) put("idempotencyKey", draft.idempotencyKey)
    }

    fun updateRequest(scenario: Scenario, expectedRevision: Long): JsonObject = buildJsonObject {
        put("scenario", buildJsonObject {
            put("id", scenario.id)
            put("label", scenario.label)
            put("description", scenario.description)
            put("enabled", scenario.enabled)
            put("continueOnError", scenario.continueOnError)
            put("triggers", triggers(scenario.triggers))
            put("conditions", conditions(scenario.conditions))
            put("steps", steps(scenario.steps))
            put("revision", scenario.revision.toString())
        })
        put("expectedRevision", expectedRevision.toString())
    }

    fun parseScenario(element: JsonElement): Scenario {
        val obj = element.jsonObject
        return Scenario(
            id = obj.string("id"),
            label = obj.string("label"),
            description = obj.string("description"),
            enabled = obj.bool("enabled"),
            revision = obj.long("revision"),
            createdUnix = obj.long("createdUnix"),
            updatedUnix = obj.long("updatedUnix"),
            triggers = obj.array("triggers").map(::parseTrigger),
            conditions = obj.array("conditions").map(::parseCondition),
            steps = obj.array("steps").map(::parseStep),
            continueOnError = obj.bool("continueOnError"),
            nextDueUnix = obj.long("nextDueUnix"),
        )
    }

    fun parseTrigger(element: JsonElement): ScenarioTrigger {
        val obj = element.jsonObject
        // Standard protobuf JSON puts the active oneof variant as a field.
        if (obj.containsKey("manual")) return ScenarioTrigger.Manual
        obj["time"]?.let { return ScenarioTrigger.Time(parseTimeTrigger(it)) }
        return ScenarioTrigger.Unknown(obj.toString())
    }

    fun parseCondition(element: JsonElement): ScenarioCondition {
        val obj = element.jsonObject
        obj["entityState"]?.let { return ScenarioCondition.EntityState(
            entityId = it.jsonObject.string("entityId"),
            state = it.jsonObject.string("state"),
        ) }
        obj["timeWindow"]?.let { return ScenarioCondition.TimeWindow(
            start = parseWallTime(it.jsonObject["start"]),
            end = parseWallTime(it.jsonObject["end"]),
            weekdays = it.jsonObject.array("weekdays").mapNotNull { day -> (day as? JsonPrimitive)?.content?.toIntOrNull() },
        ) }
        return ScenarioCondition.Unknown(obj.toString())
    }

    fun parseStep(element: JsonElement): ScenarioStep {
        val obj = element.jsonObject
        obj["catalog"]?.let { return ScenarioStep.Catalog(
            entryId = it.jsonObject.string("entryId"),
            action = it.jsonObject.string("action"),
            paramsJson = it.jsonObject.string("paramsJson").ifBlank { "{}" },
        ) }
        obj["speak"]?.let { return ScenarioStep.Speak(it.jsonObject.string("text")) }
        obj["delay"]?.let { return ScenarioStep.Delay(it.jsonObject.int("seconds")) }
        return ScenarioStep.Unknown(obj.toString())
    }

    private fun parseTimeTrigger(element: JsonElement): TimeTrigger {
        val obj = element.jsonObject
        return TimeTrigger(
            timezone = obj.string("timezone"),
            at = parseWallTime(obj["at"]),
            repeat = parseEnum(obj.string("repeat"), RepeatKind.entries, RepeatKind.UNSPECIFIED, "REPEAT_KIND_"),
            onceDate = obj.string("onceDate"),
            weekdays = obj.array("weekdays").mapNotNull { (it as? JsonPrimitive)?.content?.toIntOrNull() },
        )
    }

    private fun triggers(list: List<ScenarioTrigger>): JsonArray = buildJsonArray {
        list.forEach { trigger ->
            add(
                when (trigger) {
                    ScenarioTrigger.Manual -> buildJsonObject { put("manual", JsonObject(emptyMap())) }
                    is ScenarioTrigger.Time -> buildJsonObject { put("time", timeTrigger(trigger.trigger)) }
                    is ScenarioTrigger.Unknown -> unknown(trigger.rawJson)
                },
            )
        }
    }

    private fun conditions(list: List<ScenarioCondition>): JsonArray = buildJsonArray {
        list.forEach { condition ->
            add(
                when (condition) {
                    is ScenarioCondition.EntityState -> buildJsonObject {
                        put("entityState", buildJsonObject {
                            put("entityId", condition.entityId)
                            put("state", condition.state)
                        })
                    }
                    is ScenarioCondition.TimeWindow -> buildJsonObject {
                        put("timeWindow", buildJsonObject {
                            put("start", wallTime(condition.start))
                            put("end", wallTime(condition.end))
                            if (condition.weekdays.isNotEmpty()) {
                                put("weekdays", buildJsonArray {
                                    condition.weekdays.sorted().forEach { add(JsonPrimitive(it)) }
                                })
                            }
                        })
                    }
                    is ScenarioCondition.Unknown -> unknown(condition.rawJson)
                },
            )
        }
    }

    private fun steps(list: List<ScenarioStep>): JsonArray = buildJsonArray {
        list.forEach { step ->
            add(
                when (step) {
                    is ScenarioStep.Catalog -> buildJsonObject {
                        put("catalog", buildJsonObject {
                            put("entryId", step.entryId)
                            put("action", step.action)
                            put("paramsJson", step.paramsJson.ifBlank { "{}" })
                        })
                    }
                    is ScenarioStep.Speak -> buildJsonObject {
                        put("speak", buildJsonObject { put("text", step.text) })
                    }
                    is ScenarioStep.Delay -> buildJsonObject {
                        put("delay", buildJsonObject { put("seconds", step.seconds) })
                    }
                    is ScenarioStep.Unknown -> unknown(step.rawJson)
                },
            )
        }
    }

    private fun timeTrigger(trigger: TimeTrigger): JsonObject = buildJsonObject {
        put("timezone", trigger.timezone)
        put("at", wallTime(trigger.at))
        put("repeat", trigger.repeat.wire())
        if (trigger.onceDate.isNotBlank()) put("onceDate", trigger.onceDate)
        if (trigger.weekdays.isNotEmpty()) {
            put("weekdays", buildJsonArray { trigger.weekdays.sorted().forEach { add(JsonPrimitive(it)) } })
        }
    }

    /** Re-emits a retained unknown variant object exactly as received. */
    private fun unknown(rawJson: String): JsonObject =
        runCatching { kotlinx.serialization.json.Json.parseToJsonElement(rawJson).jsonObject }
            .getOrElse { JsonObject(emptyMap()) }

    private fun wallTime(time: WallTime): JsonObject = buildJsonObject {
        put("hour", time.hour)
        put("minute", time.minute)
        put("second", time.second)
    }

    private fun parseWallTime(element: JsonElement?): WallTime {
        if (element == null || element !is JsonObject) return WallTime(0, 0)
        var hour = element.int("hour")
        var minute = element.int("minute")
        var second = element.int("second")
        if (hour !in 0..23) hour = 0
        if (minute !in 0..59) minute = 0
        if (second !in 0..59) second = 0
        return WallTime(hour, minute, second)
    }

    private fun RepeatKind.wire(): String = "REPEAT_KIND_$name"

    private fun <T : Enum<T>> parseEnum(
        raw: String,
        values: List<T>,
        fallback: T,
        prefix: String,
    ): T = values.firstOrNull { it.name == raw || "$prefix${it.name}" == raw } ?: fallback
}
