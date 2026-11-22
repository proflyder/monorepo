package dev.proflyder.home.core.presentation

import dev.proflyder.home.core.ui.PayloadSet
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ItemDiffTest {
    private data class Row(val id: String, val title: String, val enabled: Boolean)

    private enum class RowAspect { TITLE, ENABLED }

    private val diff = KeyedDiff<Row>(
        key = Row::id,
        payload = aspectPayload(mapOf(RowAspect.TITLE to Row::title, RowAspect.ENABLED to Row::enabled)),
    )

    @Test
    fun itemsMatchByKeyAndReportOnlyChangedAspects() {
        val old = Row("1", "Lamp", enabled = false)
        val renamed = old.copy(title = "Desk lamp")
        assertTrue(diff.areItemsTheSame(old, renamed))
        assertFalse(diff.areContentsTheSame(old, renamed))
        assertEquals(PayloadSet.of(RowAspect.TITLE), diff.getChangePayload(old, renamed))
        assertEquals(ItemChanged, diff.getChangePayload(old, old.copy()))
    }

    @Test
    fun changedItemsWithoutAspectsStillCarryAPayloadSoTheRowIsReboundInPlace() {
        val plain = KeyedDiff<Row>(key = Row::id)
        assertEquals(ItemChanged, plain.getChangePayload(Row("1", "Lamp", false), Row("1", "Lamp", true)))
    }

    @Test
    fun intentCallbackTranslatesClicksIntoIntents() {
        val sent = mutableListOf<String>()
        val callback = IntentAdapterCallback<Row, String>(sent::add, onClick = { "open:${it.id}" })
        callback.onItemClick(Row("7", "Fan", enabled = true))
        callback.onItemLongClick(Row("7", "Fan", enabled = true))
        assertEquals(listOf("open:7"), sent)
    }
}
