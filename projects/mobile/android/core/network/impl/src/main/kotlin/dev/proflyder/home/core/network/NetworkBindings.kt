package dev.proflyder.home.core.network

import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.BindingContainer
import dev.zacsweers.metro.ContributesTo
import dev.zacsweers.metro.Multibinds
import dev.zacsweers.metro.Provides
import dev.zacsweers.metro.SingleIn
import okhttp3.Interceptor
import okhttp3.OkHttpClient

@ContributesTo(AppScope::class)
@BindingContainer
abstract class NetworkBindings {
    @Multibinds(allowEmpty = true)
    @NetworkInspection
    abstract fun inspectionInterceptors(): Set<Interceptor>

    @Multibinds(allowEmpty = true)
    abstract fun routeConditions(): Set<RouteCondition>

    companion object {
        @Provides
        @SingleIn(AppScope::class)
        fun provideBaseClient(): OkHttpClient = OkHttpClient.Builder()
            .followRedirects(false)
            .followSslRedirects(false)
            .retryOnConnectionFailure(true)
            .build()
    }
}
