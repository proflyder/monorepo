package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.FavoriteKind
import dev.proflyder.home.core.model.FavoriteRef
import dev.proflyder.home.core.model.Favorites
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.RepeatKind
import dev.proflyder.home.core.model.Scenario
import dev.proflyder.home.core.model.Schedule
import dev.proflyder.home.core.model.ScheduleKind
import dev.proflyder.home.core.model.Timer
import dev.proflyder.home.core.model.TimerState
import dev.proflyder.home.core.model.WallTime
import java.time.LocalDateTime
import java.time.ZoneOffset
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class HomeHubTest {
    private val now = LocalDateTime.of(2026, 11, 8, 21, 15).toEpochSecond(ZoneOffset.UTC)

    private fun epoch(day: Int, hour: Int, minute: Int): Long =
        LocalDateTime.of(2026, 11, day, hour, minute).toEpochSecond(ZoneOffset.UTC)

    private fun alarm(
        id: String,
        next: Long,
        enabled: Boolean = true,
        activeOccurrence: Long = 0,
        activeUntil: Long = 0,
    ) = Schedule(
        id = id,
        label = id,
        kind = ScheduleKind.ALARM,
        timezone = "UTC",
        at = WallTime(7, 30),
        onceDate = "",
        repeat = RepeatKind.DAILY,
        weekdays = emptyList(),
        nextDueUnix = next,
        enabled = enabled,
        snoozeUntilUnix = 0,
        lastFiredUnix = 0,
        snoozeCount = 0,
        revision = 1,
        createdUnix = 0,
        updatedUnix = 0,
        activeOccurrenceUnix = activeOccurrence,
        activeUntilUnix = activeUntil,
    )

    private fun timer(id: String, due: Long, state: TimerState = TimerState.RUNNING, remaining: Long = 0) =
        Timer(
            id = id,
            label = id,
            durationSeconds = 600,
            remainingSeconds = remaining,
            dueUnix = due,
            state = state,
            revision = 1,
            createdUnix = 1,
            updatedUnix = 1,
        )

    private fun scenario(id: String, next: Long, enabled: Boolean = true, label: String = id) = Scenario(
        id = id,
        label = label,
        description = "",
        enabled = enabled,
        revision = 1,
        createdUnix = 0,
        updatedUnix = 0,
        triggers = emptyList(),
        conditions = emptyList(),
        steps = emptyList(),
        continueOnError = false,
        nextDueUnix = next,
    )

    private fun entry(
        id: String,
        kind: ControlKind = ControlKind.LIGHT,
        title: String = id,
        device: String = "",
        available: Boolean = true,
    ) = ControlEntry(
        id = id,
        kind = kind,
        title = title,
        room = "Bedroom",
        icon = "",
        actions = when (kind) {
            ControlKind.ACTION -> setOf(ControlAction.SPEAK)
            ControlKind.SCENARIO -> setOf(ControlAction.RUN)
            else -> setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF)
        },
        power = PowerState.OFF,
        available = available,
        device = device,
    )

    // --- display names ------------------------------------------------------

    @Test
    fun technicalEntityTitlesFallBackToTheDeviceName() {
        assertEquals("Лампа", entry("light.a", title = "Light", device = "Лампа").displayName)
        assertEquals("Светильник", entry("switch.b", ControlKind.SWITCH, title = "Switch", device = "Светильник").displayName)
        assertEquals("Лампа", entry("light.c", title = "", device = "Лампа").displayName)
        // The title already is the device name: no duplicate second line.
        val same = entry("light.d", title = "Лампа", device = "Лампа")
        assertEquals("Лампа", same.displayName)
        assertNull(same.secondaryName)
    }

    @Test
    fun aMeaningfulEntityTitleStaysAndTheDeviceBecomesTheSecondLine() {
        val entry = entry("light.ceiling", title = "Ambient", device = "Ceiling lamp")
        assertEquals("Ambient", entry.displayName)
        assertEquals("Ceiling lamp", entry.secondaryName)
        // Without a device there is no second line.
        assertNull(entry("light.e", title = "Lamp").secondaryName)
    }

    // --- carousel selection -------------------------------------------------

    private val strings = object : HubScheduleStrings {
        override fun today() = "сегодня"
        override fun tomorrow() = "завтра"
        override fun dayOfWeek(isoDay: Int) = listOf("пн", "вт", "ср", "чт", "пт", "сб", "вс")[isoDay - 1]
        override fun time(hour: Int, minute: Int) = String.format("%02d:%02d", hour, minute)
    }

    @Test
    fun whenTextUsesTodayTomorrowThenWeekday() {
        assertEquals("сегодня 07:30", HubFormat.whenText(epoch(8, 7, 30), ZoneOffset.UTC, now, strings))
        assertEquals("завтра 07:30", HubFormat.whenText(epoch(9, 7, 30), ZoneOffset.UTC, now, strings))
        assertEquals("ср 07:30", HubFormat.whenText(epoch(11, 7, 30), ZoneOffset.UTC, now, strings))
    }

    @Test
    fun onlyLiveCardsAreBuiltInOrder() {
        val cards = HubCards.build(
            alarms = listOf(
                alarm("later", epoch(10, 7, 30)),
                alarm("next", epoch(9, 7, 30)),
                alarm("off", epoch(8, 22, 0), enabled = false),
            ),
            timers = listOf(
                timer("t1", now + 300),
                timer("paused", due = 0, state = TimerState.PAUSED, remaining = 120),
                timer("fired", now - 10, state = TimerState.FIRED),
            ),
            scenarios = listOf(scenario("s1", epoch(9, 8, 0)), scenario("s2", epoch(9, 7, 0))),
            reminders = listOf(alarm("r1", epoch(8, 23, 0)), alarm("r2", epoch(10, 19, 0))),
            nowUnix = now,
        )
        assertEquals(
            listOf("alarm:next", "timer:t1", "timer:paused", "scenario:s2", "reminder:r1"),
            cards.map { it.key },
        )
    }

    @Test
    fun aRingingAlarmReplacesTheNearestOneAndComesFirst() {
        val cards = HubCards.build(
            alarms = listOf(
                alarm("next", epoch(9, 7, 30)),
                alarm("ringing", epoch(8, 21, 0), activeOccurrence = epoch(8, 21, 0), activeUntil = now + 60),
            ),
            timers = emptyList(),
            scenarios = emptyList(),
            reminders = emptyList(),
            nowUnix = now,
        )
        assertEquals(listOf("alarm:ringing"), cards.map { it.key })
        assertTrue((cards.single() as HubCard.Alarm).ringing)
    }

    @Test
    fun nothingLiveMeansNoCarousel() {
        assertTrue(
            HubCards.build(
                alarms = listOf(alarm("off", 0L, enabled = false)),
                timers = emptyList(),
                scenarios = listOf(scenario("manual", 0L)),
                reminders = emptyList(),
                nowUnix = now,
            ).isEmpty(),
        )
    }

    // --- favorites ----------------------------------------------------------

    @Test
    fun pinningTogglesAndKeepsThePinningOrder() {
        val a = FavoriteRef(FavoriteKind.DEVICE, "light.a")
        val b = FavoriteRef(FavoriteKind.SCENARIO, "scenario.b")
        var pinned = Favorites.toggle(emptyList(), a)
        pinned = Favorites.toggle(pinned, b)
        assertEquals(listOf(a, b), pinned)
        assertEquals(listOf(b), Favorites.toggle(pinned, a))
        assertEquals(listOf(a, b), Favorites.toggle(Favorites.toggle(pinned, a), a).sortedBy { it.key })
    }

    @Test
    fun defaultsPinSpeakAndTheMigratedNativeScenarios() {
        val entries = listOf(
            entry("action.speak", ControlKind.ACTION, title = "Сказать на колонке"),
            entry("action.dead", ControlKind.ACTION, title = "Dead", available = false),
            entry("light.a"),
        )
        val scenarios = listOf(
            scenario("s.blackout", 1, label = "Блэкаут"),
            scenario("s.work", 1, label = "Сел работать"),
            scenario("s.other", 1, label = "Кино"),
        )
        assertEquals(
            listOf(
                FavoriteRef(FavoriteKind.ACTION, "action.speak"),
                FavoriteRef(FavoriteKind.SCENARIO, "s.blackout"),
                FavoriteRef(FavoriteKind.SCENARIO, "s.work"),
            ),
            Favorites.defaults(entries, scenarios),
        )
    }

    @Test
    fun favoritesRoundTripAndDropMalformedEntries() {
        val refs = listOf(
            FavoriteRef(FavoriteKind.DEVICE, "light.a"),
            FavoriteRef(FavoriteKind.SCENARIO, "s:with:colons"),
        )
        assertEquals(refs, Favorites.decode(Favorites.encode(refs)))
        assertEquals(emptyList<FavoriteRef>(), Favorites.decode(listOf("bogus", "device:", ":x")))
    }

    @Test
    fun quickTilesFollowPinsAndSkipMissingTargets() {
        val refs = listOf(
            FavoriteRef(FavoriteKind.DEVICE, "light.a"),
            FavoriteRef(FavoriteKind.ACTION, "action.speak"),
            FavoriteRef(FavoriteKind.SCENARIO, "s1"),
            FavoriteRef(FavoriteKind.DEVICE, "gone"),
        )
        val tiles = QuickTiles.build(
            refs = refs,
            entries = listOf(
                entry("light.a"),
                entry("action.speak", ControlKind.ACTION, title = "Сказать"),
            ),
            scenarios = listOf(scenario("s1", 1)),
        )
        assertEquals(listOf("call", "device:light.a", "action:action.speak", "scenario:s1"), tiles.map { it.key })
        assertFalse(tiles.isEmpty())
    }

    @Test
    fun failedHubSourceKeepsItsPreviousCard() {
        val due = epoch(9, 7, 30)
        val previous = HubCards.build(
            alarms = listOf(alarm("a1", due)),
            timers = listOf(timer("t1", due + 600)),
            scenarios = listOf(scenario("s1", due)),
            reminders = listOf(alarm("r1", due)),
            nowUnix = now,
        )
        // The alarm RPC failed: only the other three lists loaded.
        val cards = HubCards.buildOrKeep(
            alarms = null,
            timers = listOf(timer("t1", due + 600)),
            scenarios = listOf(scenario("s1", due)),
            reminders = listOf(alarm("r1", due)),
            previous = previous,
            nowUnix = now,
        )
        assertEquals(listOf("alarm:a1", "timer:t1", "scenario:s1", "reminder:r1"), cards.map { it.key })

        // A successful empty list still removes the card: the server said there is none.
        val emptied = HubCards.buildOrKeep(
            alarms = emptyList(),
            timers = emptyList(),
            scenarios = emptyList(),
            reminders = emptyList(),
            previous = previous,
            nowUnix = now,
        )
        assertTrue(emptied.isEmpty())
    }
}
