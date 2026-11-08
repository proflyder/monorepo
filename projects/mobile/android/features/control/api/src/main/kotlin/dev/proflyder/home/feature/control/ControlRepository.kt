package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.LightAdjust
import dev.proflyder.home.core.model.Outcome

/**
 * Domain contract for the home control catalog (scenarios and devices).
 *
 * Reading the catalog goes to the home control service; executing a command
 * goes through the scheduler, which owns execution. [send] therefore means
 * "queued on the server", not "already executed".
 */
interface ControlRepository {
    /** True when a home control address is configured. */
    val isConfigured: Boolean

    suspend fun catalog(): Outcome<ControlCatalog>

    suspend fun send(entryId: String, action: ControlAction): Outcome<Unit>

    /** Queues a light adjustment (brightness / color temperature / color). */
    suspend fun adjust(entryId: String, adjust: LightAdjust): Outcome<Unit>

    /**
     * Queues "speak this text on the station". The text is user content: it
     * only ever goes into the task payload, never into a URL or a log line.
     */
    suspend fun speak(entryId: String, text: String): Outcome<Unit>
}
