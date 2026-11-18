package dev.proflyder.home.core.network

import java.time.ZoneId
import okhttp3.Interceptor
import okhttp3.Response

/**
 * Sends the phone's IANA zone on every request to our services.
 *
 * The scheduler stores UTC only and renders schedule times in this zone; the
 * zone is read at request time so a travel or manual time-zone change takes
 * effect without restarting the app.
 */
internal class TimeZoneInterceptor(
    private val zoneId: () -> String = { ZoneId.systemDefault().id },
) : Interceptor {
    override fun intercept(chain: Interceptor.Chain): Response {
        val request = chain.request().newBuilder()
            .header(HEADER, zoneId())
            .build()
        return chain.proceed(request)
    }

    companion object {
        const val HEADER = "Time-Zone"
    }
}
