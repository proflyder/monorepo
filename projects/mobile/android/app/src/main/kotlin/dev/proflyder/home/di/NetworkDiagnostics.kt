package dev.proflyder.home.di

import android.content.Context
import android.util.Log
import com.chuckerteam.chucker.api.ChuckerCollector
import com.chuckerteam.chucker.api.ChuckerInterceptor
import com.chuckerteam.chucker.api.RetentionManager
import dev.proflyder.home.R
import okhttp3.Interceptor
import okhttp3.logging.HttpLoggingInterceptor

object NetworkDiagnostics {
    private const val LOG_TAG = "HomeHttp"
    private const val REDACTED_HEADER = "Authorization"

    fun interceptors(context: Context): Set<Interceptor> {
        if (!context.resources.getBoolean(R.bool.home_network_inspection)) return emptySet()

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
        return setOf(logging, chucker)
    }

    private const val MAX_BODY_BYTES = 250_000L
}
