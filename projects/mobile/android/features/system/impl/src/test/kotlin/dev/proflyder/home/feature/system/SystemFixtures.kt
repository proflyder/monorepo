package dev.proflyder.home.feature.system

import dev.proflyder.home.core.model.GuestKind
import dev.proflyder.home.core.model.HealthLevel
import dev.proflyder.home.core.model.KubeCluster
import dev.proflyder.home.core.model.KubeNamespace
import dev.proflyder.home.core.model.KubePod
import dev.proflyder.home.core.model.PhoneDevice
import dev.proflyder.home.core.model.PveGuest
import dev.proflyder.home.core.model.PveHost
import dev.proflyder.home.core.model.ScrapeTarget
import dev.proflyder.home.core.model.StationHealth
import dev.proflyder.home.core.model.SystemAlert
import dev.proflyder.home.core.model.SystemDevices
import dev.proflyder.home.core.model.SystemHosts
import dev.proflyder.home.core.model.SystemSections
import dev.proflyder.home.core.model.SystemSnapshot
import dev.proflyder.home.core.model.SystemSummary
import dev.proflyder.home.core.model.Usage

internal object SystemFixtures {
    private fun guest(vmid: Int, name: String, kind: GuestKind, running: Boolean, level: HealthLevel) = PveGuest(
        vmid = vmid,
        name = name,
        kind = kind,
        running = running,
        onboot = true,
        level = level,
        cpuRatio = if (running) 0.12 else null,
        memory = Usage(if (running) 2.1e9 else null, 4e9),
        disk = Usage(null, 32e9),
        uptimeSeconds = if (running) 1_490_000.0 else null,
    )

    private fun pod(name: String, level: HealthLevel, reason: String = "", ready: Int = 1) = KubePod(
        name = name,
        phase = "Running",
        ready = ready,
        containers = 1,
        restarts = if (reason.isEmpty()) 0 else 9,
        recentRestarts = if (reason.isEmpty()) 0 else 3,
        reason = reason,
        level = level,
        cpuCores = 0.004,
        memBytes = 21e6,
        ageSeconds = 86_400.0,
    )

    val snapshot = SystemSnapshot(
        summary = SystemSummary(
            status = HealthLevel.CRITICAL,
            generatedAtUnix = 1_795_129_665,
            sections = SystemSections(HealthLevel.OK, HealthLevel.CRITICAL, HealthLevel.CRITICAL, HealthLevel.OK),
            alerts = listOf(
                SystemAlert("KubePodCrashLooping", HealthLevel.CRITICAL, "currency-bot is crash looping", 1_795_100_000),
                SystemAlert("PinsStale", HealthLevel.WARNING, "Deploy pins are stale", 1_795_000_000),
            ),
            alertsAvailable = true,
        ),
        hosts = SystemHosts(
            hostLevel = HealthLevel.OK,
            guestsLevel = HealthLevel.CRITICAL,
            host = PveHost("pve", true, 0.234, Usage(10e9, 16e9), Usage(60e9, 100e9), 61.0, 1_491_534.0),
            guestsAvailable = true,
            guests = listOf(
                guest(102, "vpn", GuestKind.CT, running = false, level = HealthLevel.CRITICAL),
                guest(100, "haos", GuestKind.VM, running = true, level = HealthLevel.OK),
                guest(101, "lab-k3s", GuestKind.VM, running = true, level = HealthLevel.OK),
                guest(103, "bb", GuestKind.CT, running = true, level = HealthLevel.OK),
            ),
            targetsAvailable = true,
            targets = listOf(
                ScrapeTarget("station", 0, 1, HealthLevel.CRITICAL),
                ScrapeTarget("pve-node", 1, 1, HealthLevel.OK),
            ),
        ),
        cluster = KubeCluster(
            level = HealthLevel.CRITICAL,
            available = true,
            podCount = 3,
            problemCount = 1,
            namespaces = listOf(
                KubeNamespace("currency-bot", HealthLevel.CRITICAL, listOf(pod("currency-bot-6c", HealthLevel.CRITICAL, "CrashLoopBackOff", 0))),
                KubeNamespace("home-assistant", HealthLevel.OK, listOf(pod("scheduler-7d9", HealthLevel.OK), pod("mcp-gateway-5f", HealthLevel.OK))),
            ),
        ),
        devices = SystemDevices(
            level = HealthLevel.OK,
            station = StationHealth(true, HealthLevel.OK, "v0.5.1020", "Yandex Station Midi", 32.0, null, 58.5, 4000.0),
            phonesAvailable = true,
            phones = listOf(PhoneDevice("pixel", "Pixel", "android")),
        ),
    )

    val empty = SystemSnapshot(
        summary = SystemSummary(
            status = HealthLevel.UNKNOWN,
            generatedAtUnix = 1_795_129_665,
            sections = SystemSections(HealthLevel.UNKNOWN, HealthLevel.UNKNOWN, HealthLevel.UNKNOWN, HealthLevel.UNKNOWN),
            alerts = emptyList(),
            alertsAvailable = false,
        ),
        hosts = SystemHosts(HealthLevel.UNKNOWN, HealthLevel.UNKNOWN, null, false, emptyList()),
        cluster = KubeCluster(HealthLevel.UNKNOWN, false, 0, 0, emptyList()),
        devices = SystemDevices(HealthLevel.UNKNOWN, null, false, emptyList()),
    )
}
