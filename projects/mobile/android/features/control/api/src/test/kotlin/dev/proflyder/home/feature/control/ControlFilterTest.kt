package dev.proflyder.home.feature.control

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ControlFilterTest {
    @Test
    fun everyFilterRoundTrips() {
        listOf(
            ControlFilter.All,
            ControlFilter.Scenarios,
            ControlFilter.Lights,
            ControlFilter.Switches,
            ControlFilter.Room("Bedroom"),
            ControlFilter.Room("Кухня: свет | 1"),
            ControlFilter.Room("room:nested"),
        ).forEach { filter ->
            assertEquals(filter, ControlFilter.decode(ControlFilter.encode(filter)))
        }
    }

    @Test
    fun unknownOrMalformedValuesDecodeToNull() {
        listOf(null, "", "ALL", "devices", "room:", "room:   ", "rooms:Bedroom").forEach { value ->
            assertNull("$value", ControlFilter.decode(value))
        }
    }
}
