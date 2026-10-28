package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.ConsumerStatus
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleDraft
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.SchedulerStatus
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.model.TimerState
import dev.proflyder.home.core.model.WallTime
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/**
 * Explicit protobuf-JSON mapping for ricardo.v1.
 *
 * Enums are sent by their proto names and int64 values are decimal strings, as
 * required by the Connect JSON codec. Parsing is intentionally tolerant of
 * numeric or string int64 input because servers differ slightly in codec
 * versions, and of unknown enum values (mapped to UNSPECIFIED).
 */
object SchedulerJson {
    const val SERVICE = "ricardo.v1.SchedulerService"

    fun createScheduleRequest(draft: ScheduleDraft): JsonObject = buildJsonObject {
        put("label", draft.label)
        put("kind", draft.kind.wire())
        put("timezone", draft.timezone)
        put("at", wallTime(draft.at))
        if (draft.onceDate.isNotBlank()) put("onceDate", draft.onceDate)
        put("repeat", draft.repeat.wire())
        if (draft.weekdays.isNotEmpty()) {
            put("weekdays", buildJsonArray { draft.weekdays.sorted().forEach { add(JsonPrimitive(it)) } })
        }
        put("enabled", draft.enabled)
        if (draft.payloadJson.isNotBlank()) put("payloadJson", draft.payloadJson)
        if (draft.idempotencyKey.isNotBlank()) put("idempotencyKey", draft.idempotencyKey)
    }

    fun updateScheduleRequest(schedule: Schedule, expectedRevision: Long): JsonObject = buildJsonObject {
        put("schedule", buildJsonObject {
            put("id", schedule.id)
            put("label", schedule.label)
            put("kind", schedule.kind.wire())
            put("timezone", schedule.timezone)
            put("at", wallTime(schedule.at))
            if (schedule.onceDate.isNotBlank()) put("onceDate", schedule.onceDate)
            put("repeat", schedule.repeat.wire())
            if (schedule.weekdays.isNotEmpty()) {
                put("weekdays", buildJsonArray { schedule.weekdays.sorted().forEach { add(JsonPrimitive(it)) } })
            }
            put("enabled", schedule.enabled)
            if (schedule.payloadJson.isNotBlank()) put("payloadJson", schedule.payloadJson)
            put("revision", schedule.revision.toString())
        })
        put("expectedRevision", expectedRevision.toString())
    }

    fun parseSchedule(element: JsonElement): Schedule {
        val obj = element.jsonObject
        return Schedule(
            id = obj.string("id"),
            label = obj.string("label"),
            kind = parseEnum(obj.string("kind"), ScheduleKind.entries, ScheduleKind.UNSPECIFIED, "SCHEDULE_KIND_"),
            timezone = obj.string("timezone"),
            at = parseWallTime(obj["at"]),
            onceDate = obj.string("onceDate"),
            repeat = parseEnum(obj.string("repeat"), RepeatKind.entries, RepeatKind.UNSPECIFIED, "REPEAT_KIND_"),
            weekdays = obj.array("weekdays").mapNotNull { it.jsonPrimitive.intOrNull() },
            nextDueUnix = obj.long("nextDueUnix"),
            enabled = obj.bool("enabled"),
            snoozeUntilUnix = obj.long("snoozeUntilUnix"),
            lastFiredUnix = obj.long("lastFiredUnix"),
            snoozeCount = obj.long("snoozeCount"),
            revision = obj.long("revision"),
            createdUnix = obj.long("createdUnix"),
            updatedUnix = obj.long("updatedUnix"),
            payloadJson = obj.string("payloadJson"),
            activeOccurrenceUnix = obj.long("activeOccurrenceUnix"),
            activeUntilUnix = obj.long("activeUntilUnix"),
        )
    }

    fun parseTimer(element: JsonElement): Timer {
        val obj = element.jsonObject
        return Timer(
            id = obj.string("id"),
            label = obj.string("label"),
            durationSeconds = obj.long("durationSeconds"),
            remainingSeconds = obj.long("remainingSeconds"),
            dueUnix = obj.long("dueUnix"),
            state = parseEnum(obj.string("state"), TimerState.entries, TimerState.UNSPECIFIED, "TIMER_STATE_"),
            revision = obj.long("revision"),
            createdUnix = obj.long("createdUnix"),
            updatedUnix = obj.long("updatedUnix"),
        )
    }

    fun parseStatus(element: JsonElement): SchedulerStatus {
        val obj = element.jsonObject
        return SchedulerStatus(
            version = obj.string("version"),
            schemaVersion = obj.int("schemaVersion"),
            nowUnix = obj.long("nowUnix"),
            lastTickUnix = obj.long("lastTickUnix"),
            pendingEvents = obj.int("pendingEvents"),
            leasedEvents = obj.int("leasedEvents"),
            enabledSchedules = obj.int("enabledSchedules"),
            runningTimers = obj.int("runningTimers"),
            consumers = obj.array("consumers").map { consumer ->
                val c = consumer.jsonObject
                ConsumerStatus(
                    consumerId = c.string("consumerId"),
                    lastSeenUnix = c.long("lastSeenUnix"),
                    lastAckUnix = c.long("lastAckUnix"),
                    alive = c.bool("alive"),
                )
            },
            scenarioExecutionSupported = obj.bool("scenarioExecutionSupported"),
        )
    }

    fun parseId(element: JsonElement): String = element.jsonObject.string("id")

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

    private fun ScheduleKind.wire(): String = "SCHEDULE_KIND_$name"
    private fun RepeatKind.wire(): String = "REPEAT_KIND_$name"

    private fun <T : Enum<T>> parseEnum(
        raw: String,
        values: List<T>,
        fallback: T,
        prefix: String,
    ): T = values.firstOrNull { it.name == raw || "$prefix${it.name}" == raw } ?: fallback
}

internal fun JsonObject.string(name: String): String =
    (this[name] as? JsonPrimitive)?.contentOrNull.orEmpty()

internal fun JsonObject.long(name: String): Long =
    (this[name] as? JsonPrimitive)?.contentOrNull?.toLongOrNull() ?: 0L

internal fun JsonObject.int(name: String): Int =
    (this[name] as? JsonPrimitive)?.contentOrNull?.toIntOrNull() ?: 0

internal fun JsonObject.bool(name: String): Boolean =
    (this[name] as? JsonPrimitive)?.contentOrNull?.toBooleanStrictOrNull() ?: false

internal fun JsonObject.array(name: String): JsonArray =
    this[name] as? JsonArray ?: JsonArray(emptyList())

internal fun JsonPrimitive.intOrNull(): Int? = contentOrNull?.toIntOrNull()
