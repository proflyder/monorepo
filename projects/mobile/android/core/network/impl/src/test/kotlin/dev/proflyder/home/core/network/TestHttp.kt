package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.DeviceIdentity
import dev.proflyder.home.core.runtime.AppVersion
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import okhttp3.Interceptor
import okhttp3.OkHttpClient

internal object OpenRouteGate : RouteGate {
    override suspend fun await() = Unit
}

internal fun testHttp(
    settings: ConnectionSettingsStore,
    baseClient: OkHttpClient = OkHttpClient(),
    interceptors: Set<Interceptor> = emptySet(),
    gate: RouteGate = OpenRouteGate,
): HomeHttp = HomeHttp(
    baseClient = baseClient,
    settings = settings,
    inspection = interceptors,
    gate = gate,
    version = TestVersion,
    scope = CoroutineScope(SupervisorJob() + Dispatchers.IO),
    io = Dispatchers.IO,
    identity = TestIdentity,
)

internal object TestIdentity : DeviceIdentity {
    const val ID: String = "test-device"

    override suspend fun deviceId(): String = ID

    override fun defaultName(): String = "Test"
}

internal object TestVersion : AppVersion {
    override val name: String = "test"

    override val debuggable: Boolean = false
}
