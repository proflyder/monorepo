package dev.proflyder.home.feature.control.data

import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.LightAdjust
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.VolumeControls
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

internal data class Expectation(
    val power: PowerState? = null,
    val volume: VolumeControls? = null,
    val adjust: LightAdjust? = null,
) {
    fun satisfiedBy(entry: ControlEntry): Boolean =
        (power == null || entry.power == power) && (volume == null || entry.volume == volume)

    fun applyTo(entry: ControlEntry): ControlEntry {
        val adjusted = adjust?.let(entry::withAdjust) ?: entry
        return adjusted.copy(power = power ?: adjusted.power, volume = volume ?: adjusted.volume)
    }
}

internal data class TimedExpectation(val expectation: Expectation, val deadlineNanos: Long)

internal class OptimisticDeviceState(private val clock: Clock, private val ttlNanos: Long) {
    private val pending = MutableStateFlow<Map<String, TimedExpectation>>(emptyMap())

    val expectations: StateFlow<Map<String, TimedExpectation>> = pending.asStateFlow()

    fun expect(entryId: String, expectation: Expectation) = pending.update {
        it + (entryId to TimedExpectation(expectation, clock.elapsedNanos() + ttlNanos))
    }

    fun drop(entryId: String) = pending.update { it - entryId }

    fun isPending(entryId: String): Boolean = entryId in pending.value

    fun settle(entries: List<ControlEntry>) {
        val byId = entries.associateBy { it.id }
        val now = clock.elapsedNanos()
        pending.update { current -> current.filter { (id, timed) -> timed.stillExpected(byId[id], now) } }
    }

    fun overlay(entries: List<ControlEntry>, current: Map<String, TimedExpectation>): List<ControlEntry> {
        if (current.isEmpty()) return entries
        val now = clock.elapsedNanos()
        return entries.map { entry -> current[entry.id]?.takeIf { it.deadlineNanos > now }?.expectation?.applyTo(entry) ?: entry }
    }

    private fun TimedExpectation.stillExpected(entry: ControlEntry?, now: Long): Boolean =
        deadlineNanos > now && (entry == null || !expectation.satisfiedBy(entry))
}

internal fun ControlEntry.withAdjust(adjust: LightAdjust): ControlEntry {
    val current = light ?: return this
    return copy(
        power = PowerState.ON,
        light = current.copy(
            brightnessPct = adjust.brightnessPct ?: current.brightnessPct,
            kelvin = adjust.kelvin ?: current.kelvin,
            hue = adjust.hue ?: current.hue,
            saturation = adjust.saturation ?: current.saturation,
        ),
    )
}
