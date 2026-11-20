package dev.proflyder.home.core.network

import java.time.ZoneId
import okhttp3.Interceptor
import okhttp3.Response

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
