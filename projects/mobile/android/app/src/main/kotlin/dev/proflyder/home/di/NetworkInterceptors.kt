package dev.proflyder.home.di

import android.content.Context
import android.util.Log
import com.chuckerteam.chucker.api.ChuckerCollector
import com.chuckerteam.chucker.api.ChuckerInterceptor
import com.chuckerteam.chucker.api.RetentionManager
import dev.proflyder.home.R
import okhttp3.Interceptor
import okhttp3.logging.HttpLoggingInterceptor

/** Application interceptors added to the scheduler and control-catalog clients. */
class NetworkInterceptors(val list: List<Interceptor>)

/**
 * Network diagnostics, enabled by the `home_network_inspection` bool (a release
 * variant should override it to false so nothing below is created):
 *  - an [HttpLoggingInterceptor] (BODY) writing to logcat under "HomeHttp",
 *  - a Chucker inspector (notification + launcher shortcut "Network activity").
 *
 * The bearer token must never reach either: `Authorization` is redacted in both.
 * Chucker keeps request/response bodies in an on-device database, so retention
 * is one day and bodies are capped.
 */
object NetworkDiagnostics {
    private const val LOG_TAG = "HomeHttp"
    private const val REDACTED_HEADER = "Authorization"

    fun interceptors(context: Context): NetworkInterceptors {
        if (!context.resources.getBoolean(R.bool.home_network_inspection)) return NetworkInterceptors(emptyList())

        val logging = HttpLoggingInterceptor { message -> Log.d(LOG_TAG, message) }.apply {
            level = HttpLoggingInterceptor.Level.BODY
            redactHeader(REDACTED_HEADER)
        }
        val chucker = ChuckerInterceptor.Builder(context)
            .collector(ChuckerCollector(context, showNotification = true, retentionPeriod = RetentionManager.Period.ONE_DAY))
            .maxContentLength(MAX_BODY_BYTES)
            .redactHeaders(REDACTED_HEADER)
            .alwaysReadResponseBody(false)
            .build()
        return NetworkInterceptors(listOf(logging, chucker))
    }

    private const val MAX_BODY_BYTES = 250_000L
}
