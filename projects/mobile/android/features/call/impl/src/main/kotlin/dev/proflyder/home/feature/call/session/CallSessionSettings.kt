package dev.proflyder.home.feature.call.session

import java.util.UUID
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

internal data class CallSessionSettings(
    val watchdog: Duration = 60.seconds,
    val peerFallbackName: String = "",
    val newIdempotencyKey: () -> String = { UUID.randomUUID().toString() },
)
