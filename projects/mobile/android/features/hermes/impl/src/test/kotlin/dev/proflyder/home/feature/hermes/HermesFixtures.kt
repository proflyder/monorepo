package dev.proflyder.home.feature.hermes

import dev.proflyder.home.core.model.HermesAgent
import dev.proflyder.home.core.model.HermesAgentStatus
import dev.proflyder.home.core.model.HermesAgents
import dev.proflyder.home.core.model.HermesReport
import dev.proflyder.home.core.model.HermesReports
import dev.proflyder.home.core.model.VoiceRequest
import dev.proflyder.home.core.model.VoiceStatus

internal object HermesFixtures {
    const val NOON: Long = 1_763_640_000L

    val agents = HermesAgents(
        available = true,
        agents = listOf(
            HermesAgent("thr_1", "Почини свет в коридоре", "Station Lab", HermesAgentStatus.RUNNING, 754),
            HermesAgent("thr_2", "Конспект по английскому", "English Notes", HermesAgentStatus.IDLE, null),
            HermesAgent("thr_3", "Обнови прошивку", "Station Lab", HermesAgentStatus.FAILED, null),
        ),
    )

    val reports = HermesReports(
        available = true,
        reports = listOf(
            HermesReport("41", NOON, NOON + 34, "Дом в порядке.\nK3s: 42 пода, проблем нет.\nКолонка: Pulsar 3.4, онлайн.", false),
            HermesReport("17", NOON - 86_400, null, "", true),
        ),
    )

    val voice = VoiceStatus(
        relayUp = true,
        hermesUp = true,
        hermesVersion = "0.12.0",
        agentMode = false,
        requestsAvailable = true,
        requests = listOf(
            VoiceRequest("9", NOON + 120, "Поставь будильник на семь", "Будильник на 07:00 поставлен", 2350, false),
            VoiceRequest("7", NOON, "Какая завтра погода", null, null, true),
        ),
    )

    val state = HermesState(
        agents = Loaded(value = agents),
        reports = Loaded(value = reports),
        voice = Loaded(value = voice),
    )
}
