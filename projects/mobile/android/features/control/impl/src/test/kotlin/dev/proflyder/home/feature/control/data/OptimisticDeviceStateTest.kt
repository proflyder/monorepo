package dev.proflyder.home.feature.control.data

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.LightAdjust
import dev.proflyder.home.core.model.LightControls
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.VolumeControls
import dev.proflyder.home.core.testing.FakeClock
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class OptimisticDeviceStateTest {
    private val clock = FakeClock()
    private val state = OptimisticDeviceState(clock, ttlNanos = TTL_NANOS)

    private val lamp = ControlEntry(
        id = "light.lamp",
        kind = ControlKind.LIGHT,
        title = "Lamp",
        room = "",
        icon = "",
        actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
        power = PowerState.OFF,
        available = true,
        light = LightControls(brightness = true, colorTemp = false, color = false, brightnessPct = 10),
    )

    private fun overlay(entries: List<ControlEntry>) = state.overlay(entries, state.expectations.value)

    @Test
    fun anExpectationOverlaysTheServerStateUntilItIsMet() {
        state.expect(lamp.id, Expectation(power = PowerState.ON))
        assertEquals(PowerState.ON, overlay(listOf(lamp)).single().power)
        state.settle(listOf(lamp))
        assertTrue(state.isPending(lamp.id))
        state.settle(listOf(lamp.copy(power = PowerState.ON)))
        assertFalse(state.isPending(lamp.id))
    }

    @Test
    fun anExpiredExpectationNoLongerOverlays() {
        state.expect(lamp.id, Expectation(power = PowerState.ON))
        clock.advanceMillis(TTL_NANOS / NANOS_PER_MILLI + 1)
        assertEquals(PowerState.OFF, overlay(listOf(lamp)).single().power)
        state.settle(listOf(lamp))
        assertFalse(state.isPending(lamp.id))
    }

    @Test
    fun aLightAdjustTurnsTheLightOnWithTheNewBrightness() {
        state.expect(lamp.id, Expectation(power = PowerState.ON, adjust = LightAdjust(brightnessPct = 70)))
        val shown = overlay(listOf(lamp)).single()
        assertEquals(PowerState.ON, shown.power)
        assertEquals(70, shown.light?.brightnessPct)
    }

    @Test
    fun aVolumeExpectationIsMetOnlyByTheSameVolume() {
        val station = lamp.copy(id = "volume.station", kind = ControlKind.VOLUME, volume = VolumeControls(level = 10))
        state.expect(station.id, Expectation(volume = VolumeControls(level = 40)))
        assertEquals(40, overlay(listOf(station)).single().volume?.level)
        state.settle(listOf(station.copy(volume = VolumeControls(level = 40))))
        assertFalse(state.isPending(station.id))
    }

    private companion object {
        const val NANOS_PER_MILLI = 1_000_000L
        const val TTL_NANOS = 15_000L * NANOS_PER_MILLI
    }
}
