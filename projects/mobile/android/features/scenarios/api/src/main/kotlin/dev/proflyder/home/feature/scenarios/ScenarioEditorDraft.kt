package dev.proflyder.home.feature.scenarios

import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.ScenarioCondition
import dev.proflyder.home.core.model.ScenarioDraft
import dev.proflyder.home.core.model.ScenarioStep
import dev.proflyder.home.core.model.ScenarioTrigger
import dev.proflyder.home.core.model.TimeTrigger
import dev.proflyder.home.core.model.WallTime
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/**
 * Non-secret editor draft persisted in SavedStateHandle. It intentionally holds
 * no token, endpoint credentials, or request payload.
 *
 * [hasUnsupportedParts] is derived from the retained unknown oneof variants: a
 * definition created by a newer server can be opened and read, but saving it
 * is blocked so the phone never overwrites fields it cannot represent.
 */
data class ScenarioEditorDraft(
    val existingId: String? = null,
    val existingRevision: Long? = null,
    val label: String = "",
    val description: String = "",
    val enabled: Boolean = true,
    val continueOnError: Boolean = false,
    val triggers: List<ScenarioTrigger> = emptyList(),
    val conditions: List<ScenarioCondition> = emptyList(),
    val steps: List<ScenarioStep> = emptyList(),
) {
    val hasUnsupportedParts: Boolean
        get() = triggers.any { !it.isSupported } ||
            conditions.any { !it.isSupported } ||
            steps.any { !it.isSupported }
}

object ScenarioEditorDraftCodec {
    private const val KEY_DRAFT = "scenario_draft"
    private val json = Json { ignoreUnknownKeys = true }

    /** Stores the whole nested draft as one JSON string in SavedStateHandle. */
    fun toMap(draft: ScenarioEditorDraft): Map<String, Any?> =
        mapOf(KEY_DRAFT to encode(draft))

    /** Returns null for an absent or unreadable draft. */
    fun fromMap(values: Map<String, Any?>): ScenarioEditorDraft? {
        val raw = values[KEY_DRAFT] as? String ?: return null
        return runCatching {
            decode(json.parseToJsonElement(raw).jsonObject)
        }.getOrElse { null }
    }

    fun fromScenario(scenario: Scenario): ScenarioEditorDraft = ScenarioEditorDraft(
        existingId = scenario.id,
        existingRevision = scenario.revision,
        label = scenario.label,
        description = scenario.description,
        enabled = scenario.enabled,
        continueOnError = scenario.continueOnError,
        triggers = scenario.triggers,
        conditions = scenario.conditions,
        steps = scenario.steps,
    )

    private fun encode(draft: ScenarioEditorDraft): String = buildJsonObject {
        putBasicFields(draft)
        put("triggers", buildJsonArray { draft.triggers.forEach { add(encodeTrigger(it)) } })
        put("conditions", buildJsonArray { draft.conditions.forEach { add(encodeCondition(it)) } })
        put("steps", buildJsonArray { draft.steps.forEach { add(encodeStep(it)) } })
    }.toString()

    private fun kotlinx.serialization.json.JsonObjectBuilder.putBasicFields(draft: ScenarioEditorDraft) {
        draft.existingId?.let { put("existingId", it) }
        draft.existingRevision?.let { put("existingRevision", it.toString()) }
        put("label", draft.label)
        put("description", draft.description)
        put("enabled", draft.enabled)
        put("continueOnError", draft.continueOnError)
    }

    private fun encodeTrigger(trigger: ScenarioTrigger): JsonObject = when (trigger) {
        ScenarioTrigger.Manual -> buildJsonObject { put("kind", "manual") }
        is ScenarioTrigger.Time -> buildJsonObject {
            put("kind", "time")
            put("timezone", trigger.trigger.timezone)
            put("hour", trigger.trigger.at.hour)
            put("minute", trigger.trigger.at.minute)
            put("repeat", trigger.trigger.repeat.name)
            put("onceDate", trigger.trigger.onceDate)
            put("weekdays", buildJsonArray {
                trigger.trigger.weekdays.forEach { add(JsonPrimitive(it)) }
            })
        }
        is ScenarioTrigger.Unknown -> buildJsonObject {
            put("kind", "unknown")
            put("raw", trigger.rawJson)
        }
    }

    private fun encodeCondition(condition: ScenarioCondition): JsonObject = when (condition) {
        is ScenarioCondition.EntityState -> buildJsonObject {
            put("kind", "entity")
            put("entityId", condition.entityId)
            put("state", condition.state)
        }
        is ScenarioCondition.TimeWindow -> buildJsonObject {
            put("kind", "window")
            put("startHour", condition.start.hour)
            put("startMinute", condition.start.minute)
            put("endHour", condition.end.hour)
            put("endMinute", condition.end.minute)
            put("weekdays", buildJsonArray {
                condition.weekdays.forEach { add(JsonPrimitive(it)) }
            })
        }
        is ScenarioCondition.Unknown -> buildJsonObject {
            put("kind", "unknown")
            put("raw", condition.rawJson)
        }
    }

    private fun encodeStep(step: ScenarioStep): JsonObject = when (step) {
        is ScenarioStep.Catalog -> buildJsonObject {
            put("kind", "catalog")
            put("entryId", step.entryId)
            put("action", step.action)
            put("paramsJson", step.paramsJson)
        }
        is ScenarioStep.Speak -> buildJsonObject {
            put("kind", "speak")
            put("text", step.text)
        }
        is ScenarioStep.Delay -> buildJsonObject {
            put("kind", "delay")
            put("seconds", step.seconds)
        }
        is ScenarioStep.Unknown -> buildJsonObject {
            put("kind", "unknown")
            put("raw", step.rawJson)
        }
    }

    private fun decode(obj: JsonObject): ScenarioEditorDraft = ScenarioEditorDraft(
        existingId = obj.stringOrNull("existingId"),
        existingRevision = obj["existingRevision"]?.jsonPrimitive?.contentOrNull?.toLongOrNull(),
        label = obj.stringOrNull("label").orEmpty(),
        description = obj.stringOrNull("description").orEmpty(),
        enabled = obj["enabled"]?.jsonPrimitive?.contentOrNull?.toBooleanStrictOrNull() ?: true,
        continueOnError = obj["continueOnError"]?.jsonPrimitive?.contentOrNull?.toBooleanStrictOrNull() ?: false,
        triggers = obj.array("triggers").map(::decodeTrigger),
        conditions = obj.array("conditions").map(::decodeCondition),
        steps = obj.array("steps").map(::decodeStep),
    )

    private fun decodeTrigger(element: JsonElement): ScenarioTrigger {
        val obj = element.jsonObject
        return when (obj.stringOrNull("kind")) {
            "time" -> ScenarioTrigger.Time(
                TimeTrigger(
                    timezone = obj.stringOrNull("timezone").orEmpty(),
                    at = WallTime(obj.int("hour").coerceIn(0, 23), obj.int("minute").coerceIn(0, 59)),
                    repeat = RepeatKind.entries.firstOrNull { it.name == obj.stringOrNull("repeat") }
                        ?: RepeatKind.DAILY,
                    onceDate = obj.stringOrNull("onceDate").orEmpty(),
                    weekdays = obj.array("weekdays").mapNotNull { it.jsonPrimitive.contentOrNull?.toIntOrNull() },
                ),
            )
            "unknown" -> ScenarioTrigger.Unknown(obj.stringOrNull("raw").orEmpty())
            else -> ScenarioTrigger.Manual
        }
    }

    private fun decodeCondition(element: JsonElement): ScenarioCondition {
        val obj = element.jsonObject
        return when (obj.stringOrNull("kind")) {
            "entity" -> ScenarioCondition.EntityState(
                entityId = obj.stringOrNull("entityId").orEmpty(),
                state = obj.stringOrNull("state").orEmpty(),
            )
            "window" -> ScenarioCondition.TimeWindow(
                start = WallTime(obj.int("startHour").coerceIn(0, 23), obj.int("startMinute").coerceIn(0, 59)),
                end = WallTime(obj.int("endHour").coerceIn(0, 23), obj.int("endMinute").coerceIn(0, 59)),
                weekdays = obj.array("weekdays").mapNotNull { it.jsonPrimitive.contentOrNull?.toIntOrNull() },
            )
            else -> ScenarioCondition.Unknown(obj.stringOrNull("raw").orEmpty())
        }
    }

    private fun decodeStep(element: JsonElement): ScenarioStep {
        val obj = element.jsonObject
        return when (obj.stringOrNull("kind")) {
            "catalog" -> ScenarioStep.Catalog(
                entryId = obj.stringOrNull("entryId").orEmpty(),
                action = obj.stringOrNull("action").orEmpty(),
                paramsJson = obj.stringOrNull("paramsJson").orEmpty().ifBlank { "{}" },
            )
            "speak" -> ScenarioStep.Speak(obj.stringOrNull("text").orEmpty())
            "delay" -> ScenarioStep.Delay(obj.int("seconds"))
            else -> ScenarioStep.Unknown(obj.stringOrNull("raw").orEmpty())
        }
    }

    private fun JsonObject.stringOrNull(name: String): String? =
        (this[name] as? JsonPrimitive)?.contentOrNull

    private fun JsonObject.int(name: String): Int =
        (this[name] as? JsonPrimitive)?.contentOrNull?.toIntOrNull() ?: 0

    private fun JsonObject.array(name: String): JsonArray =
        this[name] as? JsonArray ?: JsonArray(emptyList())
}

enum class ScenarioEditorCompletion {
    SAVED,
    DELETED,
}

/** Maps the editor draft to the Create/Update payload, trimming free text. */
fun ScenarioEditorDraft.toScenarioDraft(idempotencyKey: String = ""): ScenarioDraft = ScenarioDraft(
    label = label.trim(),
    description = description.trim(),
    enabled = enabled,
    triggers = triggers,
    conditions = conditions,
    steps = steps,
    continueOnError = continueOnError,
    idempotencyKey = idempotencyKey,
)
