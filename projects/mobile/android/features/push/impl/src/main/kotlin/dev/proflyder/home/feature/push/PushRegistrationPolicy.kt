package dev.proflyder.home.feature.push

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.text.takeUtf8Bytes

internal data class PushRegistrationRequest(
    val token: String,
    val deviceName: String,
    val appVersion: String,
    val server: String,
) {
    fun record(nowMillis: Long): PushRegistrationRecord =
        PushRegistrationRecord(token, nowMillis, deviceName, appVersion, server)
}

internal object PushRegistrationPolicy {
    const val REFRESH_INTERVAL_MILLIS: Long = 24L * 60 * 60 * 1000
    const val NAME_BYTES: Int = 64

    fun isDue(last: PushRegistration, wanted: PushRegistrationRequest, nowMillis: Long): Boolean =
        when {
            last.registeredAtMillis <= 0 -> true
            last.token != wanted.token -> true
            last.registeredName != wanted.deviceName -> true
            last.appVersion != wanted.appVersion -> true
            last.server != wanted.server -> true
            nowMillis < last.registeredAtMillis -> true
            else -> nowMillis - last.registeredAtMillis > REFRESH_INTERVAL_MILLIS
        }

    fun serverOf(config: ConnectionConfig): String? =
        config.endpoint?.baseUrl()?.takeIf { config.hasToken }

    fun resultOf(published: Outcome<Unit>): PushRegistrationResult = when (published) {
        is Outcome.Success -> PushRegistrationResult.Done
        is Outcome.Failure -> PushRegistrationResult.Rejected(published.failure)
    }

    fun cleanName(raw: String, fallback: String): String =
        raw.trim().takeUtf8Bytes(NAME_BYTES).trim().ifEmpty { fallback }
}
