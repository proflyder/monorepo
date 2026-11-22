package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioCondition
import dev.proflyder.home.core.model.ScenarioDraft
import dev.proflyder.home.core.model.ScenarioStep
import dev.proflyder.home.core.model.ScenarioTrigger
import dev.proflyder.home.core.model.TimeTrigger
import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

internal object ScenarioJson {
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
        NetworkJson.decodeFromJsonElement(ScenarioDto.serializer(), element).toDomain()

    fun parseTrigger(element: JsonElement): ScenarioTrigger {
        val obj = element.jsonObject
        return when {
            obj.containsKey("manual") -> ScenarioTrigger.Manual
            obj.containsKey("time") -> ScenarioTrigger.Time(
                NetworkJson.decodeFromJsonElement(TimeTriggerDto.serializer(), element.required("time"))
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
                NetworkJson.decodeFromJsonElement(SpeakStepDto.serializer(), element.required("speak")).text,
            )
            obj.containsKey("delay") -> ScenarioStep.Delay(
                NetworkJson.decodeFromJsonElement(DelayStepDto.serializer(), element.required("delay")).seconds,
            )
            else -> ScenarioStep.Unknown(obj.toString())
        }
    }

    private fun entityStateCondition(element: JsonElement): ScenarioCondition.EntityState =
        NetworkJson.decodeFromJsonElement(EntityStateConditionDto.serializer(), element).let {
            ScenarioCondition.EntityState(entityId = it.entityId, state = it.state)
        }

    private fun timeWindowCondition(element: JsonElement): ScenarioCondition.TimeWindow =
        NetworkJson.decodeFromJsonElement(TimeWindowConditionDto.serializer(), element).let {
            ScenarioCondition.TimeWindow(
                start = (it.start ?: WallTimeDto()).toDomain(),
                end = (it.end ?: WallTimeDto()).toDomain(),
                weekdays = it.weekdays,
            )
        }

    private fun catalogStep(element: JsonElement): ScenarioStep.Catalog =
        NetworkJson.decodeFromJsonElement(CatalogStepDto.serializer(), element).let {
            ScenarioStep.Catalog(
                entryId = it.entryId,
                action = it.action,
                paramsJson = it.paramsJson.ifBlank { "{}" },
            )
        }

    private fun triggerJson(trigger: ScenarioTrigger): JsonObject = when (trigger) {
        ScenarioTrigger.Manual -> encode(TriggerInputDto.serializer(), TriggerInputDto(manual = JsonObject(emptyMap())))
        is ScenarioTrigger.Time -> encode(TriggerInputDto.serializer(), TriggerInputDto(time = trigger.trigger.input()))
        is ScenarioTrigger.Unknown -> unknown(trigger.rawJson)
    }

    private fun conditionJson(condition: ScenarioCondition): JsonObject = when (condition) {
        is ScenarioCondition.EntityState -> encode(
            ConditionInputDto.serializer(),
            ConditionInputDto(entityState = EntityStateConditionDto(condition.entityId, condition.state)),
        )
        is ScenarioCondition.TimeWindow -> encode(ConditionInputDto.serializer(), ConditionInputDto(timeWindow = condition.input()))
        is ScenarioCondition.Unknown -> unknown(condition.rawJson)
    }

    private fun stepJson(step: ScenarioStep): JsonObject = when (step) {
        is ScenarioStep.Catalog -> encode(
            StepInputDto.serializer(),
            StepInputDto(catalog = CatalogStepDto(step.entryId, step.action, step.paramsJson.ifBlank { "{}" })),
        )
        is ScenarioStep.Speak -> encode(StepInputDto.serializer(), StepInputDto(speak = SpeakStepDto(step.text)))
        is ScenarioStep.Delay -> encode(StepInputDto.serializer(), StepInputDto(delay = DelayStepDto(step.seconds)))
        is ScenarioStep.Unknown -> unknown(step.rawJson)
    }

    private fun TimeTrigger.input(): TimeTriggerInputDto = TimeTriggerInputDto(
        timezone = timezone,
        at = at.dto(),
        repeat = repeat.dto(),
        onceDate = onceDate.takeIf { it.isNotBlank() },
        weekdays = weekdays.sorted().takeIf { it.isNotEmpty() },
    )

    private fun ScenarioCondition.TimeWindow.input(): TimeWindowInputDto = TimeWindowInputDto(
        start = start.dto(),
        end = end.dto(),
        weekdays = weekdays.sorted().takeIf { it.isNotEmpty() },
    )

    private fun <T> encode(serializer: KSerializer<T>, value: T): JsonObject =
        NetworkJson.encodeToJsonElement(serializer, value).jsonObject

    private fun unknown(rawJson: String): JsonObject = NetworkJson.decodeFromString(
        JsonObject.serializer(),
        rawJson.ifBlank { "{}" },
    )

    private fun JsonElement.required(field: String): JsonElement =
        jsonObject.getValue(field)
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

@Serializable
internal data class TriggerInputDto(
    val manual: JsonObject? = null,
    val time: TimeTriggerInputDto? = null,
)

@Serializable
internal data class TimeTriggerInputDto(
    val timezone: String,
    val at: WallTimeDto,
    val repeat: RepeatKindDto,
    val onceDate: String? = null,
    val weekdays: List<Int>? = null,
)

@Serializable
internal data class ConditionInputDto(
    val entityState: EntityStateConditionDto? = null,
    val timeWindow: TimeWindowInputDto? = null,
)

@Serializable
internal data class TimeWindowInputDto(
    val start: WallTimeDto,
    val end: WallTimeDto,
    val weekdays: List<Int>? = null,
)

@Serializable
internal data class StepInputDto(
    val catalog: CatalogStepDto? = null,
    val speak: SpeakStepDto? = null,
    val delay: DelayStepDto? = null,
)
