package dev.proflyder.home.feature.call

import dev.proflyder.home.feature.call.telecom.CallTelecomBridge

/**
 * Entry point for framework-instantiated components (`ConnectionService`,
 * `Activity`, foreground service) that Metro cannot construct. The application
 * installs the graph dependencies once at launch; this is not a general
 * service locator.
 */
object CallEntryPoint {
    @Volatile
    private var dependencies: Dependencies? = null

    fun install(dependencies: Dependencies) {
        this.dependencies = dependencies
    }

    fun require(): Dependencies = checkNotNull(dependencies) {
        "CallEntryPoint is not installed; HomeApplication must call install()"
    }

    data class Dependencies(
        val controller: CallController,
        val audioController: CallAudioController,
        val coordinator: CallCoordinator,
        val telecomBridge: CallTelecomBridge,
    )
}
