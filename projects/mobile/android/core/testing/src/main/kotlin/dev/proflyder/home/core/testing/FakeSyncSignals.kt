package dev.proflyder.home.core.testing

import dev.proflyder.home.core.data.SyncSignal
import dev.proflyder.home.core.data.SyncSignalSink
import dev.proflyder.home.core.data.SyncSignals
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow

class FakeSyncSignals : SyncSignals, SyncSignalSink {
    private val bus = MutableSharedFlow<SyncSignal>(extraBufferCapacity = 16)

    override val signals: SharedFlow<SyncSignal> = bus

    override fun publish(signal: SyncSignal) {
        bus.tryEmit(signal)
    }
}
