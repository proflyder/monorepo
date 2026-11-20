package dev.proflyder.home.feature.chat.data

import dev.proflyder.home.core.model.ChatAttachmentKind
import dev.proflyder.home.core.model.ChatQuote
import dev.proflyder.home.core.model.ChatRole
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class OutboxCodecTest {
    @Test
    fun recordsRoundTripWithQuotesAndFailures() {
        val records = listOf(
            Outgoing("c1", "Привет", null, 1L).toRecord(),
            Outgoing("c2", "Да", ChatQuote("m2", ChatRole.ASSISTANT, "Свет?"), 2L, "chat_unauthorized").toRecord(),
            Outgoing(
                "c3",
                "",
                null,
                3L,
                attachments = listOf(
                    LocalAttachment("/data/v.wav", ChatAttachmentKind.VOICE, "audio/wav", "v.wav", 64, 1200, "f_1"),
                    LocalAttachment("/data/p.jpg", ChatAttachmentKind.IMAGE, "image/jpeg", "p.jpg", 99),
                ),
            ).toRecord(),
        )
        val decoded = OutboxCodec.decode(OutboxCodec.encode(records)).map { it.toOutgoing() }
        assertEquals(records.map { it.toOutgoing() }, decoded)
        assertEquals(emptyList<OutgoingRecord>(), OutboxCodec.decode("not json"))
    }
}
