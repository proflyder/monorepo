package dev.proflyder.home.feature.call.telecom

import android.os.Build
import android.os.OutcomeReceiver
import android.telecom.CallEndpoint
import android.telecom.CallEndpointException
import android.telecom.Connection
import androidx.annotation.RequiresApi
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.util.concurrent.Executor
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Connects the live self-managed [Connection] with the audio router.
 *
 * On API 34+ Telecom reports the endpoints that can carry the call and applies
 * a change through the [Connection]. Older platforms have no such API, so the
 * audio router falls back to `AudioManager`.
 */
@SingleIn(AppScope::class)
@Inject
class CallTelecomBridge {

    private var connection: Connection? = null
    private val mutableEndpoints = MutableStateFlow<List<Int>>(emptyList())
    private val mutableSelectedEndpoint = MutableStateFlow<Int?>(null)

    /** Endpoint types (`CallEndpoint.TYPE_*`) currently offered by Telecom. */
    val endpointTypes: StateFlow<List<Int>> = mutableEndpoints.asStateFlow()

    val selectedEndpointType: StateFlow<Int?> = mutableSelectedEndpoint.asStateFlow()

    fun attach(connection: Connection) {
        this.connection = connection
    }

    fun detach(connection: Connection) {
        if (this.connection === connection) {
            this.connection = null
            mutableEndpoints.value = emptyList()
            mutableSelectedEndpoint.value = null
        }
    }

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    fun onAvailableEndpoints(endpoints: List<CallEndpoint>) {
        mutableEndpoints.value = endpoints.map { it.endpointType }
    }

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    fun onSelectedEndpoint(endpoint: CallEndpoint) {
        mutableSelectedEndpoint.value = endpoint.endpointType
    }

    /**
     * Requests an endpoint change through Telecom. Returns false when the
     * platform or the connection cannot honour it, so the caller can fall back
     * to `AudioManager`.
     */
    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    fun requestEndpoint(type: Int, executor: Executor, onResult: (Boolean) -> Unit): Boolean {
        val current = connection ?: return false
        val target = lastEndpoints.firstOrNull { it.endpointType == type } ?: return false
        current.requestCallEndpointChange(
            target,
            executor,
            object : OutcomeReceiver<Void, CallEndpointException> {
                override fun onResult(result: Void?) = onResult(true)
                override fun onError(error: CallEndpointException) = onResult(false)
            },
        )
        return true
    }

    private var lastEndpoints: List<CallEndpoint> = emptyList()

    @RequiresApi(Build.VERSION_CODES.UPSIDE_DOWN_CAKE)
    fun rememberEndpoints(endpoints: List<CallEndpoint>) {
        lastEndpoints = endpoints
        onAvailableEndpoints(endpoints)
    }
}
