package dev.proflyder.home.core.network

import dev.proflyder.home.core.testing.TestFixtures
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** DTO codec round-trips and the int64-as-string contract. */
class WireDtosTest {

    @Test
    fun scheduleDtoEncodesInt64AsStringAndRoundTrips() {
        val dto = ScheduleDto(
            id = "s1",
            kind = ScheduleKindDto.ALARM,
            repeat = RepeatKindDto.CUSTOM,
            weekdays = listOf(1, 3, 5),
            nextDueUnix = 9_000,
            revision = 7,
        )
        val encoded = WireJson.encodeToString(ScheduleDto.serializer(), dto)
        assertTrue(encoded, encoded.contains("\"revision\":\"7\""))
        assertTrue(encoded, encoded.contains("\"nextDueUnix\":\"9000\""))
        assertEquals(dto, WireJson.decodeFromString(ScheduleDto.serializer(), encoded))
    }

    @Test
    fun int64SerializerAcceptsNumbersAndStrings() {
        val fromString = WireJson.decodeFromString(
            TimerDto.serializer(),
            TestFixtures.read("dto_timer_string.json"),
        )
        val fromNumber = WireJson.decodeFromString(
            TimerDto.serializer(),
            TestFixtures.read("dto_timer_number.json"),
        )
        assertEquals(42L, fromString.dueUnix)
        assertEquals(42L, fromNumber.dueUnix)
    }

    @Test
    fun intercomEnumsUseProtoNames() {
        val call = WireJson.decodeFromString(
            CallDto.serializer(),
            TestFixtures.read("dto_call_active.json"),
        )
        assertEquals(IntercomCallStateDto.ACTIVE, call.state)
        assertEquals(IntercomEndReasonDto.BUSY, call.endReason)
        val encoded = WireJson.encodeToString(CallDto.serializer(), call)
        assertTrue(encoded, encoded.contains("\"state\":\"CALL_STATE_ACTIVE\""))
    }

    @Test
    fun unknownEnumValueFallsBackToUnspecified() {
        val call = WireJson.decodeFromString(
            CallDto.serializer(),
            TestFixtures.read("dto_call_unknown_state.json"),
        )
        assertEquals(IntercomCallStateDto.UNSPECIFIED, call.state)
    }
}
