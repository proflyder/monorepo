package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioCondition
import dev.proflyder.home.core.model.ScenarioDraft
import dev.proflyder.home.core.model.ScenarioStep
import dev.proflyder.home.core.model.ScenarioTrigger
import dev.proflyder.home.core.model.TimeTrigger
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

/**
 * Maps home.v1.ScenarioService DTOs to domain models and builds typed requests.
 *
 * The oneof active variant is a field of the message object (`{"time":{...}}`).
 * A variant this client does not know is retained verbatim as its original JSON
 * object so an older app round-trips a newer definition without data loss.
 */
object ScenarioJson {
    const val SERVICE = "home.v1.ScenarioService"

    fun createRequest(draft: ScenarioDraft): CreateScenarioRequestDto = CreateScenarioRequestDto(
        label = draft.label,
        description = draft.description,
        enabled = draft.enabled,
        continueOnError = draft.continueOnError,
        triggers = draft.triggers.map(::triggerJson),
        conditions = draft.conditions.map(::conditionJson),
        steps = draft.steps.map(::stepJson),
        idempotencyKey = draft.idempotencyKey.takeIf { it.isNotBlank() },
    )

    fun updateRequest(scenario: Scenario, expectedRevision: Long): UpdateScenarioRequestDto =
        UpdateScenarioRequestDto(
            scenario = ScenarioInputDto(
                id = scenario.id,
                label = scenario.label,
                description = scenario.description,
                enabled = scenario.enabled,
                continueOnError = scenario.continueOnError,
                triggers = scenario.triggers.map(::triggerJson),
                conditions = scenario.conditions.map(::conditionJson),
                steps = scenario.steps.map(::stepJson),
                revision = scenario.revision,
            ),
            expectedRevision = expectedRevision,
        )

    fun parseScenario(element: JsonElement): Scenario =
        WireJson.decodeFromJsonElement(ScenarioDto.serializer(), element).toDomain()

    fun parseTrigger(element: JsonElement): ScenarioTrigger {
        val obj = element.jsonObject
        return when {
            obj.containsKey("manual") -> ScenarioTrigger.Manual
            obj.containsKey("time") -> ScenarioTrigger.Time(
                WireJson.decodeFromJsonElement(TimeTriggerDto.serializer(), element.required("time"))
                    .toDomain(),
            )
            else -> ScenarioTrigger.Unknown(obj.toString())
        }
    }

    fun parseCondition(element: JsonElement): ScenarioCondition {
        val obj = element.jsonObject
        return when {
            obj.containsKey("entityState") -> entityStateCondition(element.required("entityState"))
            obj.containsKey("timeWindow") -> timeWindowCondition(element.required("timeWindow"))
            else -> ScenarioCondition.Unknown(obj.toString())
        }
    }

    fun parseStep(element: JsonElement): ScenarioStep {
        val obj = element.jsonObject
        return when {
            obj.containsKey("catalog") -> catalogStep(element.required("catalog"))
            obj.containsKey("speak") -> ScenarioStep.Speak(
                WireJson.decodeFromJsonElement(SpeakStepDto.serializer(), element.required("speak")).text,
            )
            obj.containsKey("delay") -> ScenarioStep.Delay(
                WireJson.decodeFromJsonElement(DelayStepDto.serializer(), element.required("delay")).seconds,
            )
            else -> ScenarioStep.Unknown(obj.toString())
        }
    }

    private fun entityStateCondition(element: JsonElement): ScenarioCondition.EntityState =
        WireJson.decodeFromJsonElement(EntityStateConditionDto.serializer(), element).let {
            ScenarioCondition.EntityState(entityId = it.entityId, state = it.state)
        }

    private fun timeWindowCondition(element: JsonElement): ScenarioCondition.TimeWindow =
        WireJson.decodeFromJsonElement(TimeWindowConditionDto.serializer(), element).let {
            ScenarioCondition.TimeWindow(
                start = (it.start ?: WallTimeDto()).toDomain(),
                end = (it.end ?: WallTimeDto()).toDomain(),
                weekdays = it.weekdays,
            )
        }

    private fun catalogStep(element: JsonElement): ScenarioStep.Catalog =
        WireJson.decodeFromJsonElement(CatalogStepDto.serializer(), element).let {
            ScenarioStep.Catalog(
                entryId = it.entryId,
                action = it.action,
                paramsJson = it.paramsJson.ifBlank { "{}" },
            )
        }

    private fun triggerJson(trigger: ScenarioTrigger): JsonObject = when (trigger) {
        ScenarioTrigger.Manual -> buildJsonObject { put("manual", JsonObject(emptyMap())) }
        is ScenarioTrigger.Time -> buildJsonObject { put("time", timeTriggerJson(trigger.trigger)) }
        is ScenarioTrigger.Unknown -> unknown(trigger.rawJson)
    }

    private fun conditionJson(condition: ScenarioCondition): JsonObject = when (condition) {
        is ScenarioCondition.EntityState -> buildJsonObject {
            put("entityState", buildJsonObject {
                put("entityId", condition.entityId)
                put("state", condition.state)
            })
        }
        is ScenarioCondition.TimeWindow -> buildJsonObject {
            put("timeWindow", buildJsonObject {
                put("start", condition.start.dto().json())
                put("end", condition.end.dto().json())
                if (condition.weekdays.isNotEmpty()) {
                    put("weekdays", buildJsonArray {
                        condition.weekdays.sorted().forEach { add(JsonPrimitive(it)) }
                    })
                }
            })
        }
        is ScenarioCondition.Unknown -> unknown(condition.rawJson)
    }

    private fun stepJson(step: ScenarioStep): JsonObject = when (step) {
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
    }

    private fun timeTriggerJson(trigger: TimeTrigger): JsonObject = buildJsonObject {
        put("timezone", trigger.timezone)
        put("at", trigger.at.dto().json())
        put("repeat", trigger.repeat.dto().wire())
        if (trigger.onceDate.isNotBlank()) put("onceDate", trigger.onceDate)
        if (trigger.weekdays.isNotEmpty()) {
            put("weekdays", buildJsonArray {
                trigger.weekdays.sorted().forEach { add(JsonPrimitive(it)) }
            })
        }
    }

    /** Re-emits a retained unknown variant object exactly as received. */
    private fun unknown(rawJson: String): JsonObject = WireJson.decodeFromString(
        JsonObject.serializer(),
        rawJson.ifBlank { "{}" },
    )

    private fun JsonElement.required(field: String): JsonElement =
        jsonObject.getValue(field)

    private fun WallTimeDto.json(): JsonObject = buildJsonObject {
        put("hour", hour)
        put("minute", minute)
        put("second", second)
    }

    private fun RepeatKindDto.wire(): String = "REPEAT_KIND_$name"
}

internal fun ScenarioDto.toDomain(): Scenario = Scenario(
    id = id,
    label = label,
    description = description,
    enabled = enabled,
    revision = revision,
    createdUnix = createdUnix,
    updatedUnix = updatedUnix,
    nextDueUnix = nextDueUnix,
    triggers = triggers.map(ScenarioJson::parseTrigger),
    conditions = conditions.map(ScenarioJson::parseCondition),
    steps = steps.map(ScenarioJson::parseStep),
    continueOnError = continueOnError,
)

internal fun TimeTriggerDto.toDomain(): TimeTrigger = TimeTrigger(
    timezone = timezone,
    at = (at ?: WallTimeDto()).toDomain(),
    repeat = repeat.toDomain(),
    onceDate = onceDate,
    weekdays = weekdays,
)
