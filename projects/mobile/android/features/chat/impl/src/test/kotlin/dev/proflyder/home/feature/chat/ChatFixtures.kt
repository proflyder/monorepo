package dev.proflyder.home.feature.chat

import dev.proflyder.home.core.model.ChatKind
import dev.proflyder.home.core.model.ChatMessage
import dev.proflyder.home.core.model.ChatQuote
import dev.proflyder.home.core.model.ChatRole
import dev.proflyder.home.core.model.ChatStatus

internal object ChatFixtures {
    const val NOON = 1_795_129_665_000L
    private const val MINUTE = 60_000L

    fun message(
        id: String,
        seq: Long,
        role: ChatRole,
        text: String,
        status: ChatStatus = ChatStatus.DONE,
        clientId: String? = null,
        replyTo: ChatQuote? = null,
        version: Long = seq,
        error: String? = null,
    ): ChatMessage = ChatMessage(
        id = id,
        seq = seq,
        version = version,
        clientId = clientId,
        role = role,
        kind = ChatKind.TEXT,
        text = text,
        replyTo = replyTo,
        status = status,
        errorKey = error,
        createdAtMillis = NOON + seq * MINUTE,
        updatedAtMillis = NOON + seq * MINUTE,
    )

    val conversation: List<ChatMessage> = listOf(
        message("m1", 1, ChatRole.USER, "Включи свет в гостиной", clientId = "c1"),
        message("m2", 2, ChatRole.ASSISTANT, "Готово: **свет в гостиной** включён."),
        message(
            "m3",
            3,
            ChatRole.USER,
            "А что с сервисами?",
            clientId = "c2",
            replyTo = ChatQuote("m2", ChatRole.ASSISTANT, "Готово: **свет в гостиной** включён."),
        ),
        message(
            "m4",
            4,
            ChatRole.ASSISTANT,
            "Всё в норме, кроме одного:\n\n- `scheduler` — 2 рестарта за час\n- Pulsar отвечает\n\nПодробности: https://grafana.home.example.com/d/k8s",
        ),
        message("m5", 5, ChatRole.USER, "Перезапусти scheduler", clientId = "c3"),
        message("m6", 6, ChatRole.ASSISTANT, "Перезапуск", status = ChatStatus.FAILED, error = "chat_upstream_failed"),
    )
}
