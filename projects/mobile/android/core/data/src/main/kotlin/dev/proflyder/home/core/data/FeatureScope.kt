package dev.proflyder.home.core.data

import kotlin.time.Duration
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.launch

class FeatureLifetime(parent: CoroutineScope) : AutoCloseable {
    private val job = SupervisorJob(parent.coroutineContext[Job])

    val scope: CoroutineScope = CoroutineScope(parent.coroutineContext + job)

    override fun close() = job.cancel()
}

class Lease<G : Any>(val graph: G, private val release: () -> Unit) : AutoCloseable {
    private var released = false

    override fun close() {
        synchronized(this) {
            if (released) return
            released = true
        }
        release()
    }
}

interface FeatureScope<G : Any> {
    val owners: Int

    val instance: G?

    fun acquire(): Lease<G>
}

class RefCountedScope<G : Any>(
    private val host: CoroutineScope,
    background: Flow<Boolean>,
    private val releaseDelay: Duration,
    private val create: (FeatureLifetime) -> G,
) : FeatureScope<G> {
    private val lock = Any()
    private var live: Live<G>? = null
    private var pendingRelease: Job? = null

    override var owners: Int = 0
        private set

    override val instance: G? get() = synchronized(lock) { live?.graph }

    init {
        background.filter { it }.onEach { clearIfIdle() }.launchIn(host)
    }

    override fun acquire(): Lease<G> = Lease(synchronized(lock, ::takeOwner), ::release)

    private fun takeOwner(): G {
        pendingRelease?.cancel()
        owners += 1
        val current = live ?: FeatureLifetime(host).let { Live(create(it), it) }.also { live = it }
        return current.graph
    }

    private fun release() {
        synchronized(lock) {
            owners -= 1
            if (owners > 0) return
            pendingRelease = host.launch {
                delay(releaseDelay)
                clearIfIdle()
            }
        }
    }

    private fun clearIfIdle() {
        synchronized(lock) {
            if (owners > 0) return
            live?.lifetime?.close()
            live = null
        }
    }

    private class Live<G>(val graph: G, val lifetime: FeatureLifetime)
}

fun <G : Any, T : Any> FeatureScope<G>.map(project: (G) -> T): FeatureScope<T> = ProjectedScope(this, project)

private class ProjectedScope<G : Any, T : Any>(
    private val source: FeatureScope<G>,
    private val project: (G) -> T,
) : FeatureScope<T> {
    override val owners: Int get() = source.owners

    override val instance: T? get() = source.instance?.let(project)

    override fun acquire(): Lease<T> {
        val lease = source.acquire()
        return Lease(project(lease.graph), lease::close)
    }
}
