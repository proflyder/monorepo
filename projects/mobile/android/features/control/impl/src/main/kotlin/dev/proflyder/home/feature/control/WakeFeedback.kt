package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind

internal enum class WakeFeedbackKind {
    FALSE_ALARM,

    MISSED,
}

internal data class WakeFeedbackAction(
    val kind: WakeFeedbackKind,
    val entry: ControlEntry,
    val busy: Boolean,
)

internal object WakeFeedback {
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
