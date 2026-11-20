package dev.proflyder.home.feature.chat

import dev.proflyder.home.core.model.ChatAlert
import dev.proflyder.home.core.model.ChatAttachment
import dev.proflyder.home.core.model.ChatAttachmentKind
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

    val voice: ChatMessage = message("m10", 10, ChatRole.USER, "", clientId = "c10").copy(
        kind = ChatKind.VOICE,
        attachments = listOf(ChatAttachment("f_v", ChatAttachmentKind.VOICE, "audio/wav", 64_044, "voice.wav", 7_400)),
        transcript = "Включи свет в гостиной и поставь музыку",
    )

    val log: ChatMessage = message("m12", 12, ChatRole.USER, "Что не так в этом логе?", clientId = "c12").copy(
        kind = ChatKind.FILE,
        attachments = listOf(ChatAttachment("f_l", ChatAttachmentKind.FILE, "text/plain", 18_432, "scheduler.log", null)),
    )

    fun alert(id: String, seq: Long, severity: String, resolved: Boolean): ChatMessage =
        message(id, seq, ChatRole.SYSTEM, "🔴 KubePodCrashLooping\n🔎 a1b2c3d4e5f6").copy(
            kind = ChatKind.ALERT,
            alert = ChatAlert(
                fingerprint = "a1b2c3d4e5f60718",
                name = if (severity == "critical") "KubePodCrashLooping" else "PinsStale",
                severity = severity,
                resolved = resolved,
                summary = if (severity == "critical") "scheduler перезапускается" else "Пины образов старше суток",
                description = if (resolved) "" else "3 рестарта за 10 минут, последний — OOMKilled",
                namespace = "home-assistant",
                target = "scheduler-7d9",
                startsAtMillis = NOON + seq * MINUTE,
                endsAtMillis = if (resolved) NOON + (seq + 30) * MINUTE else null,
                runbookUrl = "https://runbooks.example.com/crash",
            ),
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
