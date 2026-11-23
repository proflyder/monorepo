package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.DeviceIdentity
import kotlinx.coroutines.runBlocking
import okhttp3.Interceptor
import okhttp3.Response

internal class DeviceOriginInterceptor(private val identity: DeviceIdentity) : Interceptor {
    private val deviceId: String? by lazy { runCatching { runBlocking { identity.deviceId() } }.getOrNull() }

    override fun intercept(chain: Interceptor.Chain): Response {
        val id = deviceId?.takeIf { it.isNotBlank() } ?: return chain.proceed(chain.request())
        return chain.proceed(chain.request().newBuilder().header(HEADER, id).build())
    }

    companion object {
        const val HEADER = "Home-Device-Id"
    }
}
