package dev.proflyder.home.diagnostics

import android.content.Context
import android.util.Log
import com.chuckerteam.chucker.api.ChuckerCollector
import com.chuckerteam.chucker.api.ChuckerInterceptor
import com.chuckerteam.chucker.api.RetentionManager
import dev.proflyder.home.core.network.NetworkInspection
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.BindingContainer
import dev.zacsweers.metro.ContributesTo
import dev.zacsweers.metro.IntoSet
import dev.zacsweers.metro.Provides
import dev.zacsweers.metro.SingleIn
import okhttp3.Interceptor
import okhttp3.logging.HttpLoggingInterceptor

@ContributesTo(AppScope::class)
@BindingContainer
object NetworkInspectionBindings {
    private const val LOG_TAG = "HomeHttp"
    private const val REDACTED_HEADER = "Authorization"
    private const val MAX_BODY_BYTES = 250_000L

    @Provides
    @IntoSet
    @NetworkInspection
    @SingleIn(AppScope::class)
    fun provideLogging(): Interceptor = HttpLoggingInterceptor { message -> Log.d(LOG_TAG, message) }.apply {
        level = HttpLoggingInterceptor.Level.BODY
        redactHeader(REDACTED_HEADER)
    }

    @Provides
    @IntoSet
    @NetworkInspection
    @SingleIn(AppScope::class)
    fun provideChucker(context: Context): Interceptor = ChuckerInterceptor.Builder(context)
        .collector(ChuckerCollector(context, showNotification = true, retentionPeriod = RetentionManager.Period.ONE_DAY))
        .maxContentLength(MAX_BODY_BYTES)
        .redactHeaders(REDACTED_HEADER)
        .alwaysReadResponseBody(false)
        .build()
}
