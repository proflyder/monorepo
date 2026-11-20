package dev.proflyder.home.feature.call

import dev.proflyder.home.feature.call.telecom.CallTelecomBridge

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
