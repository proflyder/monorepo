package dev.proflyder.home.core.model

/**
 * Domain models mirroring the home.v1 ScenarioService contract.
 *
 * The contract extends each `oneof` without breaking existing definitions, so
 * every trigger, condition and step keeps an [Unknown] variant. The original
 * protobuf-JSON object is retained verbatim; that lets an older app round-trip
 * a newer definition instead of silently dropping what it does not know.
 */
data class TimeTrigger(
    val timezone: String,
    val at: WallTime,
    val repeat: RepeatKind,
    val onceDate: String = "",
    val weekdays: List<Int> = emptyList(),
)

sealed interface ScenarioTrigger {
    data object Manual : ScenarioTrigger

    data class Time(val trigger: TimeTrigger) : ScenarioTrigger

    /** A trigger kind this build does not implement; [rawJson] is the original JSON object. */
    data class Unknown(val rawJson: String) : ScenarioTrigger

    val isSupported: Boolean get() = this !is Unknown
}

sealed interface ScenarioCondition {
    data class EntityState(val entityId: String, val state: String) : ScenarioCondition

    data class TimeWindow(
        val start: WallTime,
        val end: WallTime,
        val weekdays: List<Int> = emptyList(),
    ) : ScenarioCondition

    /** A condition kind this build does not implement; [rawJson] is the original JSON object. */
    data class Unknown(val rawJson: String) : ScenarioCondition

    val isSupported: Boolean get() = this !is Unknown
}

sealed interface ScenarioStep {
    /** Same entry/action/params form as the `home.control` queue task. */
    data class Catalog(
        val entryId: String,
        val action: String,
        val paramsJson: String = "{}",
    ) : ScenarioStep

    data class Speak(val text: String) : ScenarioStep

    data class Delay(val seconds: Int) : ScenarioStep

    /** A step kind this build does not implement; [rawJson] is the original JSON object. */
    data class Unknown(val rawJson: String) : ScenarioStep

    val isSupported: Boolean get() = this !is Unknown
}

data class Scenario(
    val id: String,
    val label: String,
    val description: String,
    val enabled: Boolean,
    val revision: Long,
    val createdUnix: Long,
    val updatedUnix: Long,
    val triggers: List<ScenarioTrigger>,
    val conditions: List<ScenarioCondition>,
    val steps: List<ScenarioStep>,
    val continueOnError: Boolean,
    /** Earliest time trigger; 0 when disabled, manual-only or exhausted. */
    val nextDueUnix: Long,
) {
    /** True when any part of the definition is newer than this build. */
    val hasUnsupportedParts: Boolean
        get() = triggers.any { !it.isSupported } ||
            conditions.any { !it.isSupported } ||
            steps.any { !it.isSupported }
}

/** Input for CreateScenario/UpdateScenario. */
data class ScenarioDraft(
    val label: String,
    val description: String = "",
    val enabled: Boolean = true,
    val triggers: List<ScenarioTrigger> = emptyList(),
    val conditions: List<ScenarioCondition> = emptyList(),
    val steps: List<ScenarioStep> = emptyList(),
    val continueOnError: Boolean = false,
    val idempotencyKey: String = "",
)

/** Server limits for one scenario definition. */
object ScenarioLimits {
    const val MAX_LABEL_BYTES = 80
    const val MAX_DESCRIPTION_BYTES = 512
    const val MAX_TRIGGERS = 8
    const val MAX_CONDITIONS = 8
    const val MAX_STEPS = 32
    const val MAX_SPEAK_CHARS = 500
    const val MIN_DELAY_SECONDS = 1
    const val MAX_DELAY_SECONDS = 3600
    const val MAX_ENTITY_ID_BYTES = 255
    const val MAX_ENTITY_STATE_BYTES = 255
    const val MAX_ENTRY_ID_BYTES = 200
    const val MAX_ACTION_BYTES = 64
}
