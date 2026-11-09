package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind

/** The two one-tap marks of a live voice-activator wake event. */
enum class WakeFeedbackKind {
    /** The activator fired but no one spoke to it. */
    FALSE_ALARM,

    /** The activator did not hear the wake phrase at all. */
    MISSED,
}

/** One mark resolved against the loaded catalog. */
data class WakeFeedbackAction(
    val kind: WakeFeedbackKind,
    val entry: ControlEntry,
    val busy: Boolean,
)

/**
 * Resolves the two Home Assistant scripts that mark a wake event from the
 * catalog. They are ordinary personal scenarios whose entity ids are part of
 * the house contract (`projects/home-assistant/ha-voice/scripts.yaml`), so they
 * are matched by id and never by their user-visible title. The section is
 * hidden when neither script is exposed; a single exposed script still shows
 * its own button.
 */
object WakeFeedback {
    const val FALSE_ALARM_ENTRY_ID = "script.wake_false_alarm"
    const val MISSED_ENTRY_ID = "script.wake_missed"

    private val ORDER = listOf(
        FALSE_ALARM_ENTRY_ID to WakeFeedbackKind.FALSE_ALARM,
        MISSED_ENTRY_ID to WakeFeedbackKind.MISSED,
    )

    fun actions(entries: List<ControlEntry>, pending: Set<String> = emptySet()): List<WakeFeedbackAction> {
        val byId = entries.associateBy { it.id }
        return ORDER.mapNotNull { (id, kind) -> byId[id]?.toAction(kind, pending) }
    }

    private fun ControlEntry.toAction(kind: WakeFeedbackKind, pending: Set<String>): WakeFeedbackAction? =
        if (this.kind != ControlKind.SCENARIO) null else WakeFeedbackAction(kind, this, id in pending)
}
