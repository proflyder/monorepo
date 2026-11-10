package dev.proflyder.home.feature.control

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.ControlRole
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.VolumeControls
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ControlItemsTest {
    private val texts = ControlTexts(
        otherDevices = "Other devices",
        scenarios = "Scenarios",
        unavailable = "Unavailable",
        voice = "Voice",
        onSummary = { on, total -> "$on of $total on" },
    )

    private fun entry(
        id: String,
        kind: ControlKind = ControlKind.LIGHT,
        room: String = "",
        power: PowerState? = null,
        available: Boolean = true,
        title: String = id,
        deviceId: String = "",
        device: String = "",
        role: ControlRole = ControlRole.PRIMARY,
    ) = ControlEntry(
        id = id,
        kind = kind,
        title = title,
        room = room,
        icon = "",
        actions = when (kind) {
            ControlKind.SCENARIO -> setOf(ControlAction.RUN)
            ControlKind.ACTION -> setOf(ControlAction.SPEAK)
            ControlKind.BUTTON -> setOf(ControlAction.PRESS)
            else -> setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF)
        },
        power = power,
        available = available,
        deviceId = deviceId,
        device = device,
        role = role,
    )

    private val sample = listOf(
        entry("scene.movie", ControlKind.SCENARIO),
        entry("light.a", room = "Bedroom", power = PowerState.ON),
        entry("light.b", room = "Bedroom", power = PowerState.OFF),
        entry("switch.fan", ControlKind.SWITCH, room = "Workspace", power = PowerState.OFF),
        entry("light.hall", power = PowerState.ON),
        entry("light.dead", room = "Bedroom", available = false),
    )

    private fun build(
        entries: List<ControlEntry> = sample,
        filter: ControlFilter = ControlFilter.All,
        pending: Set<String> = emptySet(),
        statesOk: Boolean = true,
        expandedDevices: Set<String> = emptySet(),
    ) = ControlItems.build(entries, pending, statesOk, filter, texts, expandedDevices)

    private fun List<ControlItem>.describe(): List<String> =
        filterNot { it is ControlItem.Filters }.map {
            when (it) {
                is ControlItem.Header -> "# ${it.title}" + (it.summary?.let { s -> " ($s)" } ?: "")
                is ControlItem.Voice -> "V profile=${it.profile?.entry?.id ?: ""}" +
                    " wake=${it.actions.joinToString(",") { action -> action.kind.name }}"
                is ControlItem.Scenario -> "S ${it.entry.id}"
                is ControlItem.Device -> "D ${it.entry.id}"
                is ControlItem.DeviceGroup ->
                    "G ${it.title} expanded=${it.expanded}" +
                        " p=[${it.primary.joinToString(",") { row -> row.entry.id }}]" +
                        " s=[${it.settings.joinToString(",") { row -> row.entry.id }}]"
                is ControlItem.Hub -> "H ${it.cards.joinToString(",") { card -> card.key }}"
                is ControlItem.QuickActions -> "Q ${it.tiles.joinToString(",") { tile -> tile.key }}"
                is ControlItem.Filters -> "F ${it.chips.joinToString(",") { chip -> chip.label }}"
            }
        }

    @Test
    fun filterChipsSitBetweenQuickActionsAndTheRoomSections() {
        val items = ControlItems.build(
            entries = sample,
            pending = emptySet(),
            statesOk = true,
            filter = ControlFilter.All,
            texts = texts,
            leading = listOf(ControlItem.QuickActions(emptyList(), hint = true)),
        )
        val kinds = items.map {
            when (it) {
                is ControlItem.QuickActions -> "quick"
                is ControlItem.Filters -> "filters"
                is ControlItem.Header -> "header:${it.section}"
                is ControlItem.Voice -> "voice"
                else -> "row"
            }
        }
        assertEquals(listOf("quick", "filters", "header:room:Bedroom"), kinds.take(3))
        val chips = items.filterIsInstance<ControlItem.Filters>().single().chips
        assertEquals(listOf("All · 5", "Lights · 4", "Switches · 1", "Bedroom · 3", "Workspace · 1"), chips.map { it.label })
    }

    @Test
    fun optionsListDeviceCategoriesThenRoomsWithCounts() {
        val options = ControlItems.options(sample)
        assertEquals(
            listOf(
                ControlFilterOption(ControlFilter.All, 5),
                ControlFilterOption(ControlFilter.Lights, 4),
                ControlFilterOption(ControlFilter.Switches, 1),
                ControlFilterOption(ControlFilter.Room("Bedroom"), 3),
                ControlFilterOption(ControlFilter.Room("Workspace"), 1),
            ),
            options,
        )
    }

    @Test
    fun roomChipsAreOmittedWhenThereIsOnlyOneRoom() {
        val options = ControlItems.options(listOf(entry("light.a", room = "Bedroom"), entry("light.b", room = "Bedroom")))
        assertEquals(listOf(ControlFilter.All, ControlFilter.Lights), options.map { it.filter })
        assertTrue(ControlItems.options(emptyList()).isEmpty())
    }

    @Test
    fun allShowsRoomsWithOnCountsThenUnavailable() {
        assertEquals(
            listOf(
                "# Bedroom (1 of 2 on)", "D light.a", "D light.b",
                "# Workspace (0 of 1 on)", "D switch.fan",
                "# Other devices (1 of 1 on)", "D light.hall",
                "# Unavailable (1)", "D light.dead",
            ),
            build().describe(),
        )
    }

    @Test
    fun aStoredScenariosFilterFallsBackToAllBecauseHomeHasNoScripts() {
        assertEquals(build(filter = ControlFilter.All).describe(), build(filter = ControlFilter.Scenarios).describe())
        assertFalse(ControlItems.options(sample).any { it.filter == ControlFilter.Scenarios })
    }

    @Test
    fun categoryAndRoomFiltersNarrowTheList() {
        assertEquals(
            listOf("# Workspace (0 of 1 on)", "D switch.fan"),
            build(filter = ControlFilter.Switches).describe(),
        )
        assertEquals(
            listOf("# Bedroom (1 of 2 on)", "D light.a", "D light.b", "# Unavailable (1)", "D light.dead"),
            build(filter = ControlFilter.Room("Bedroom")).describe(),
        )
    }

    @Test
    fun onSummaryIsOmittedWhenHomeAssistantStatesAreUnknown() {
        val headers = build(statesOk = false).filterIsInstance<ControlItem.Header>()
        assertNull(headers.first { it.section == "room:Bedroom" }.summary)
        assertFalse(build(statesOk = false).filterIsInstance<ControlItem.Device>().any { it.statesKnown })
    }

    @Test
    fun pendingEntriesAreMarkedBusy() {
        val items = build(pending = setOf("light.a"))
        assertTrue(items.filterIsInstance<ControlItem.Device>().single { it.entry.id == "light.a" }.busy)
        assertFalse(items.filterIsInstance<ControlItem.Device>().single { it.entry.id == "light.b" }.busy)
    }

    @Test
    fun aStaleFilterFallsBackToAllInsteadOfAnEmptyList() {
        val gone = ControlFilter.Room("Garage")
        assertEquals(ControlFilter.All, ControlItems.effectiveFilter(sample, gone))
        assertEquals(build(filter = ControlFilter.All).describe(), build(filter = gone).describe())
        // A category that no longer exists also falls back.
        val onlyLights = listOf(entry("light.a"), entry("light.b"))
        assertEquals(ControlFilter.All, ControlItems.effectiveFilter(onlyLights, ControlFilter.Switches))
        // Still-valid choices are kept, and so is any choice while nothing is loaded yet.
        assertEquals(ControlFilter.Lights, ControlItems.effectiveFilter(sample, ControlFilter.Lights))
        assertEquals(gone, ControlItems.effectiveFilter(emptyList(), gone))
    }

    @Test
    fun aRoomNamedLikeASectionDoesNotCollideWithIt() {
        val entries = listOf(
            entry("light.a", room = "Scenarios"),
            entry("light.b", room = "Unavailable"),
            entry("light.c", room = "Other devices"),
            entry("light.hall"),
            entry("light.dead", available = false),
        )
        val keys = build(entries).map { it.key }
        assertEquals("keys must be unique: $keys", keys.size, keys.toSet().size)
        val headers = build(entries).filterIsInstance<ControlItem.Header>()
        assertEquals(
            listOf("room:Scenarios", "room:Unavailable", "room:Other devices", "other", "unavailable"),
            headers.map { it.section },
        )
    }

    // --- the voice profile selector -----------------------------------------

    private val profile = entry("switch.voice_profile", ControlKind.PROFILE, power = PowerState.OFF)
    private val withProfile = listOf(profile) + sample

    @Test
    fun profileIsPinnedAboveDevices() {
        assertEquals(
            listOf(
                "# Voice", "V profile=switch.voice_profile wake=",
                "# Bedroom (1 of 2 on)", "D light.a", "D light.b",
                "# Workspace (0 of 1 on)", "D switch.fan",
                "# Other devices (1 of 1 on)", "D light.hall",
                "# Unavailable (1)", "D light.dead",
            ),
            build(withProfile).describe(),
        )
        // Its section and key never collide with a room or another section.
        val keys = build(withProfile).map { it.key }
        assertEquals(keys.size, keys.toSet().size)
        assertEquals(listOf("voice", "room:Bedroom", "room:Workspace", "other", "unavailable"),
            build(withProfile).filterIsInstance<ControlItem.Header>().map { it.section })
    }

    @Test
    fun profileIsNotAFilterableCategoryAndNotCounted() {
        // Chips describe content only: the profile adds no chip and no count.
        assertEquals(ControlItems.options(sample), ControlItems.options(withProfile))
        // It stays visible whatever the filter is.
        assertEquals(
            listOf("# Voice", "V profile=switch.voice_profile wake=", "# Workspace (0 of 1 on)", "D switch.fan"),
            build(withProfile, filter = ControlFilter.Switches).describe(),
        )
        assertEquals(
            listOf(
                "# Voice", "V profile=switch.voice_profile wake=",
                "# Bedroom (1 of 2 on)", "D light.a", "D light.b",
                "# Unavailable (1)", "D light.dead",
            ),
            build(withProfile, filter = ControlFilter.Room("Bedroom")).describe(),
        )
    }

    @Test
    fun profileAloneIsNotAnEmptyList() {
        val items = build(listOf(profile))
        assertEquals(listOf("# Voice", "V profile=switch.voice_profile wake="), items.describe())
        // Nothing filterable: only the "All" chip exists, with a zero count.
        assertEquals(listOf(ControlFilterOption(ControlFilter.All, 0)), ControlItems.options(listOf(profile)))
        // A stored category with nothing to show falls back to All; with no
        // entries at all the stored choice is kept for validation later.
        assertEquals(ControlFilter.All, ControlItems.effectiveFilter(listOf(profile), ControlFilter.Scenarios))
        assertEquals(ControlFilter.Scenarios, ControlItems.effectiveFilter(emptyList(), ControlFilter.Scenarios))
    }

    @Test
    fun voiceCarriesTheBusyFlagAndDevicesKeepTheirKnownState() {
        val idle = build(withProfile).filterIsInstance<ControlItem.Voice>().single().profile
        assertFalse(requireNotNull(idle).busy)
        val busy = build(withProfile, pending = setOf("switch.voice_profile"))
            .filterIsInstance<ControlItem.Voice>().single().profile
        assertTrue(requireNotNull(busy).busy)
        // Devices are unaffected by the voice block.
        assertTrue(build(withProfile, statesOk = false).filterIsInstance<ControlItem.Device>().all { !it.statesKnown })
    }

    // --- device grouping ------------------------------------------------------

    private val ceilingLamp = listOf(
        entry("light.ceiling", room = "Bedroom", power = PowerState.ON, deviceId = "dev.ceiling", device = "Ceiling lamp"),
        entry("light.ceiling.ambient", room = "Bedroom", power = PowerState.OFF, deviceId = "dev.ceiling", device = "Ceiling lamp"),
        entry(
            "switch.ceiling.nightlight",
            kind = ControlKind.SWITCH,
            room = "Bedroom",
            power = PowerState.OFF,
            deviceId = "dev.ceiling",
            device = "Ceiling lamp",
            role = ControlRole.SETTING,
        ),
    )

    @Test
    fun entriesOfOneDeviceBecomeOneGroupWithPrimaryAndSettings() {
        assertEquals(
            listOf(
                "# Bedroom (1 of 2 on)",
                "G Ceiling lamp expanded=false p=[light.ceiling,light.ceiling.ambient] s=[switch.ceiling.nightlight]",
            ),
            build(ceilingLamp).describe(),
        )
    }

    @Test
    fun aSinglePrimaryWithoutSettingsStaysAPlainRow() {
        val single = listOf(
            entry("light.desk", room = "Office", deviceId = "dev.desk", device = "Desk lamp"),
        )
        assertEquals(listOf("# Office (0 of 1 on)", "D light.desk"), build(single).describe())
        // A standalone row is titled by its device, never by the short entity
        // title or the id fallback.
        listOf("", "Light", "light.desk").forEach { shortTitle ->
            val row = build(listOf(entry("light.desk", title = shortTitle, deviceId = "dev.desk", device = "Desk lamp")))
                .filterIsInstance<ControlItem.Device>()
                .single()
            assertEquals("Desk lamp", row.entry.title)
        }
    }

    @Test
    fun aSinglePrimaryWithSettingsIsStillAGroup() {
        val entries = listOf(
            entry("light.desk", room = "Office", deviceId = "dev.desk", device = "Desk lamp", power = PowerState.OFF),
            entry(
                "switch.desk.timer",
                kind = ControlKind.SWITCH,
                room = "Office",
                deviceId = "dev.desk",
                device = "Desk lamp",
                role = ControlRole.SETTING,
            ),
        )
        assertEquals(
            listOf(
                "# Office (0 of 1 on)",
                "G Desk lamp expanded=false p=[light.desk] s=[switch.desk.timer]",
            ),
            build(entries, filter = ControlFilter.All).describe(),
        )
    }

    @Test
    fun aLampRowTakesTheDeviceName() {
        // The server's entity title is the short "Light"; a standalone device
        // row is titled by the device instead.
        val lamp = listOf(entry("light.lamp", title = "Light", deviceId = "dev.lamp", device = "Лампа"))
        assertEquals("Лампа", build(lamp).filterIsInstance<ControlItem.Device>().single().entry.title)
    }

    @Test
    fun aSinglePrimaryDeviceCardUsesTheDeviceNameForItsMainRow() {
        val settings = (1..6).map { number ->
            entry(
                "switch.strip.$number",
                kind = ControlKind.SWITCH,
                title = "Setting $number",
                deviceId = "dev.strip",
                device = "Светильник",
                role = ControlRole.SETTING,
            )
        }
        val entries = listOf(entry("light.strip", title = "Light", deviceId = "dev.strip", device = "Светильник")) + settings
        val group = build(entries).filterIsInstance<ControlItem.DeviceGroup>().single()
        assertEquals("Светильник", group.title)
        assertEquals(1, group.primary.size)
        // The lone primary is the card's main row, so its title is the device,
        // not the duplicated short "Light".
        assertEquals("Светильник", group.primary.single().entry.title)
        // Settings keep the short names the server sent.
        assertEquals(settings.map { it.id }, group.settings.map { it.entry.id })
        assertEquals((1..6).map { "Setting $it" }, group.settings.map { it.entry.title })
    }

    @Test
    fun aMultiPrimaryDeviceKeepsTheDeviceHeaderAndShortRowNames() {
        val entries = listOf(
            entry("media.mic", kind = ControlKind.SWITCH, title = "Microphones", deviceId = "dev.speaker", device = "Колонка"),
            entry("media.clock", kind = ControlKind.SWITCH, title = "Clock display", deviceId = "dev.speaker", device = "Колонка"),
            entry("media.ring", kind = ControlKind.SWITCH, title = "Ring", deviceId = "dev.speaker", device = "Колонка"),
        )
        val group = build(entries).filterIsInstance<ControlItem.DeviceGroup>().single()
        assertEquals("Колонка", group.title)
        assertEquals(listOf("Microphones", "Clock display", "Ring"), group.primary.map { it.entry.title })
    }

    @Test
    fun settingsDoNotAddChipsOrCounts() {
        // One primary light and one setting switch: no "Switches" chip appears
        // and the light is counted once.
        assertEquals(
            listOf(ControlFilterOption(ControlFilter.All, 2), ControlFilterOption(ControlFilter.Lights, 2)),
            ControlItems.options(ceilingLamp),
        )
        // The settings follow their visible device even when that chip matches
        // only the primary controls.
        assertEquals(
            listOf(
                "# Bedroom (1 of 2 on)",
                "G Ceiling lamp expanded=false p=[light.ceiling,light.ceiling.ambient] s=[switch.ceiling.nightlight]",
            ),
            build(ceilingLamp, filter = ControlFilter.Lights).describe(),
        )
    }

    @Test
    fun expandedSettingsAreDrivenByTheViewModelState() {
        val collapsed = build(ceilingLamp).filterIsInstance<ControlItem.DeviceGroup>().single()
        assertFalse(collapsed.expanded)
        val expanded = build(ceilingLamp, expandedDevices = setOf("dev.ceiling"))
            .filterIsInstance<ControlItem.DeviceGroup>()
            .single()
        assertTrue(expanded.expanded)
    }

    @Test
    fun buttonsJoinTheirDeviceByEntityIdPrefixAsSettings() {
        val entries = listOf(
            entry("switch.komp", ControlKind.SWITCH, room = "Office", title = "Комп", power = PowerState.ON),
            entry("button.komp_lock", ControlKind.BUTTON, room = "Office", title = "Заблокировать"),
            entry("button.komp_unlock", ControlKind.BUTTON, room = "Office", title = "Разблокировать"),
        )
        assertEquals(
            listOf(
                "# Office (1 of 1 on)",
                "G Комп expanded=false p=[switch.komp] s=[button.komp_lock,button.komp_unlock]",
            ),
            build(entries).describe(),
        )
    }

    @Test
    fun prefixGroupingNeedsABaseEntityAndTheSameRoom() {
        // No base entity: `lamp_a`/`lamp_b` have no `lamp` anchor, so they stay
        // separate rows even though they share a prefix.
        val withoutAnchor = listOf(
            entry("switch.lamp_a", ControlKind.SWITCH, room = "Office"),
            entry("button.lamp_b", ControlKind.BUTTON, room = "Office"),
        )
        assertEquals(
            listOf("# Office (0 of 2 on)", "D switch.lamp_a", "D button.lamp_b"),
            build(withoutAnchor).describe(),
        )
        // A base entity in a different room is not the same device.
        val acrossRooms = listOf(
            entry("switch.komp", ControlKind.SWITCH, room = "Office"),
            entry("button.komp_lock", ControlKind.BUTTON, room = "Bedroom"),
        )
        assertEquals(
            listOf("# Office (0 of 1 on)", "D switch.komp", "# Bedroom (0 of 1 on)", "D button.komp_lock"),
            build(acrossRooms).describe(),
        )
    }

    @Test
    fun entriesWithoutADeviceIdAreEachTheirOwnDevice() {
        // An older server omits device_id, so the flat rows stay exactly as before.
        assertEquals(
            listOf("# Bedroom (1 of 2 on)", "D light.a", "D light.b"),
            build(
                listOf(
                    entry("light.a", room = "Bedroom", power = PowerState.ON),
                    entry("light.b", room = "Bedroom", power = PowerState.OFF),
                ),
            ).describe(),
        )
    }

    @Test
    fun aGroupedDeviceKeepsItsBusyFlagsAndKnownState() {
        val items = build(ceilingLamp, pending = setOf("light.ceiling"), statesOk = false)
        val group = items.filterIsInstance<ControlItem.DeviceGroup>().single()
        assertTrue(group.primary.single { it.entry.id == "light.ceiling" }.busy)
        assertFalse(group.primary.single { it.entry.id == "light.ceiling.ambient" }.busy)
        assertFalse(group.statesKnown)
    }

    @Test
    fun leadingHubAndQuickActionsPrecedeTheProfileAndRooms() {
        val leading = listOf(
            ControlItem.Hub(emptyList(), emptyMap(), 0),
            ControlItem.QuickActions(emptyList(), hint = true),
        )
        assertEquals(
            listOf(
                "H ", "Q ",
                "# Bedroom (1 of 2 on)", "D light.a", "D light.b",
                "# Workspace (0 of 1 on)", "D switch.fan",
                "# Other devices (1 of 1 on)", "D light.hall",
                "# Unavailable (1)", "D light.dead",
            ),
            buildWith(leading = leading).describe(),
        )
    }

    @Test
    fun collapsedRoomsHideTheirDevicesButKeepTheHeader() {
        assertEquals(
            listOf(
                "# Bedroom (1 of 2 on)",
                "# Workspace (0 of 1 on)", "D switch.fan",
                "# Other devices (1 of 1 on)", "D light.hall",
                "# Unavailable (1)", "D light.dead",
            ),
            buildWith(collapsedRooms = setOf("room:Bedroom")).describe(),
        )
        val header = buildWith(collapsedRooms = setOf("room:Bedroom"))
            .filterIsInstance<ControlItem.Header>()
            .first { it.section == "room:Bedroom" }
        assertTrue(header.collapsible)
        assertTrue(header.collapsed)
    }

    @Test
    fun haScenariosAndDeviceActionsAreNotRenderedOnHome() {
        val entries = listOf(
            entry("action.station_speak", ControlKind.ACTION, room = "Bedroom"),
            entry("scene.movie", ControlKind.SCENARIO, room = "Bedroom"),
        )
        assertEquals(emptyList<String>(), build(entries).describe())
        assertEquals(listOf(ControlFilterOption(ControlFilter.All, 0)), ControlItems.options(entries))
    }

    private fun buildWith(
        leading: List<ControlItem> = emptyList(),
        collapsedRooms: Set<String> = emptySet(),
    ) = ControlItems.build(
        entries = sample,
        pending = emptySet(),
        statesOk = true,
        filter = ControlFilter.All,
        texts = texts,
        leading = leading,
        collapsedRooms = collapsedRooms,
    )

    @Test
    fun stationVolumeRowsJoinTheirDeviceCard() {
        val stationVolume = ControlEntry(
            id = "volume.station",
            kind = ControlKind.VOLUME,
            title = "Volume",
            room = "Bedroom",
            icon = "volume-high",
            actions = setOf(ControlAction.SET, ControlAction.MUTE, ControlAction.UNMUTE),
            power = null,
            available = true,
            volume = VolumeControls(level = 45, muted = false),
            deviceId = "station-1",
            device = "Station",
        )
        val stationMic = entry(
            "switch.station_mic",
            ControlKind.SWITCH,
            room = "Bedroom",
            power = PowerState.OFF,
            deviceId = "station-1",
            device = "Station",
        )
        val group = build(listOf(stationMic, stationVolume))
            .filterIsInstance<ControlItem.DeviceGroup>()
            .single()
        assertEquals("Station", group.title)
        assertEquals(
            listOf("switch.station_mic", "volume.station"),
            group.primary.map { it.entry.id },
        )
        assertEquals(2, ControlItems.options(listOf(stationMic, stationVolume)).first().count)
    }
}
