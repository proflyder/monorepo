package dev.proflyder.home.feature.push

internal data class PushRegistrationRequest(
    val token: String,
    val deviceName: String,
    val appVersion: String,
)

internal object PushRegistrationPolicy {
    const val REFRESH_INTERVAL_MILLIS: Long = 24L * 60 * 60 * 1000

    fun isDue(last: PushRegistration, wanted: PushRegistrationRequest, nowMillis: Long): Boolean =
        when {
            last.registeredAtMillis <= 0 -> true
            last.token != wanted.token -> true
            last.registeredName != wanted.deviceName -> true
            last.appVersion != wanted.appVersion -> true
            nowMillis < last.registeredAtMillis -> true
            else -> nowMillis - last.registeredAtMillis > REFRESH_INTERVAL_MILLIS
        }
}
