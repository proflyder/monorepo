package dev.proflyder.home.feature.system.presentation

import android.content.Context
import android.text.format.DateFormat
import android.text.format.Formatter
import dev.proflyder.home.core.model.GuestKind
import dev.proflyder.home.core.model.HealthLevel
import dev.proflyder.home.core.model.KubeCluster
import dev.proflyder.home.core.model.KubeNamespace
import dev.proflyder.home.core.model.KubePod
import dev.proflyder.home.core.model.PveGuest
import dev.proflyder.home.core.model.PveHost
import dev.proflyder.home.core.model.SystemAlert
import dev.proflyder.home.core.model.SystemDevices
import dev.proflyder.home.core.model.SystemHosts
import dev.proflyder.home.core.model.SystemSnapshot
import dev.proflyder.home.core.model.SystemSummary
import dev.proflyder.home.core.model.Usage
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.feature.access.AccessRoutes
import dev.proflyder.home.feature.hermes.HermesRoutes
import dev.proflyder.home.feature.secrets.SecretsRoutes
import dev.proflyder.home.feature.system.SystemHealthDestination
import dev.proflyder.home.feature.system.impl.R
import java.util.Date
import kotlin.math.roundToInt

internal class SystemRows(private val context: Context) {
    private val separator get() = context.getString(R.string.system_separator)

    fun overview(snapshot: SystemSnapshot, links: SystemLinks = SystemLinks()): List<SystemRow> = buildList {
        add(healthEntry(snapshot))
        if (links.access) add(accessCard())
        if (links.secrets) add(secretsCard())
        if (links.hermes) add(hermesCard())
    }

    fun health(snapshot: SystemSnapshot): List<SystemRow> = buildList {
        add(status(snapshot))
        addAll(alerts(snapshot))
        add(hostCard(snapshot.hosts))
        add(guestsCard(snapshot.hosts))
        add(clusterCard(snapshot.cluster))
        add(devicesCard(snapshot.devices))
    }

    private fun healthEntry(snapshot: SystemSnapshot): SystemRow.Link = SystemRow.Link(
        key = "health",
        title = context.getString(R.string.system_health_title),
        subtitle = join(context.getString(statusTitle(snapshot.summary.status)), alertsText(snapshot.summary)),
        route = Route.Screen(SystemHealthDestination),
        level = snapshot.summary.status,
    )

    private fun hermesCard(): SystemRow.Link = SystemRow.Link(
        key = "hermes",
        title = context.getString(R.string.system_card_hermes),
        subtitle = context.getString(R.string.system_card_hermes_subtitle),
        route = Route.Link(HermesRoutes.OVERVIEW),
    )

    private fun accessCard(): SystemRow.Link = SystemRow.Link(
        key = "access",
        title = context.getString(R.string.system_card_access),
        subtitle = context.getString(R.string.system_card_access_subtitle),
        route = Route.Link(AccessRoutes.TOKENS),
    )

    private fun secretsCard(): SystemRow.Link = SystemRow.Link(
        key = "secrets",
        title = context.getString(R.string.system_card_secrets),
        subtitle = context.getString(R.string.system_card_secrets_subtitle),
        route = Route.Link(SecretsRoutes.CATALOG),
    )

    fun detail(section: SystemSection, snapshot: SystemSnapshot): List<SystemRow> = when (section) {
        SystemSection.HOST -> hostDetail(snapshot.hosts)
        SystemSection.GUESTS -> guestsDetail(snapshot.hosts)
        SystemSection.CLUSTER -> clusterDetail(snapshot.cluster)
        SystemSection.DEVICES -> devicesDetail(snapshot.devices)
    }

    private fun status(snapshot: SystemSnapshot): SystemRow.Status {
        val summary = snapshot.summary
        val time = DateFormat.getTimeFormat(context).format(Date(summary.generatedAtUnix * MILLIS))
        val subtitle = join(alertsText(summary), context.getString(R.string.system_updated_at, time))
        return SystemRow.Status(summary.status, context.getString(statusTitle(summary.status)), subtitle)
    }

    private fun statusTitle(level: HealthLevel): Int = when (level) {
        HealthLevel.OK -> R.string.system_status_ok
        HealthLevel.WARNING -> R.string.system_status_warning
        HealthLevel.CRITICAL -> R.string.system_status_critical
        HealthLevel.UNKNOWN -> R.string.system_status_unknown
    }

    private fun alertsText(summary: SystemSummary): String = when {
        !summary.alertsAvailable -> context.getString(R.string.system_alerts_unavailable)
        else -> context.resources.getQuantityString(R.plurals.system_alert_count, summary.alerts.size, summary.alerts.size)
    }

    private fun alerts(snapshot: SystemSnapshot): List<SystemRow> =
        snapshot.summary.alerts.mapIndexed { index, alert -> alertRow(index, alert) }

    private fun alertRow(index: Int, alert: SystemAlert): SystemRow.Item {
        val since = alert.startedAtUnix?.let { context.getString(R.string.system_since, dateTime(it)) }
        return SystemRow.Item("alert:$index:${alert.name}", alert.severity, alert.summary, join(alert.name, since))
    }

    private fun hostCard(hosts: SystemHosts): SystemRow.Card {
        val host = hosts.host
        val lines = when {
            host == null -> context.getString(R.string.system_no_data_host)
            !host.up -> context.getString(R.string.system_host_down)
            else -> hostLines(host)
        }
        return SystemRow.Card(SystemSection.HOST, hosts.hostLevel, context.getString(R.string.system_card_host), lines)
    }

    private fun hostLines(host: PveHost): String {
        val load = join(
            host.cpuRatio?.let { context.getString(R.string.system_cpu_short, percent(it)) },
            host.memory.ratio?.let { context.getString(R.string.system_mem_short, percent(it)) },
            host.disk.ratio?.let { context.getString(R.string.system_disk_short, percent(it)) },
        )
        val health = join(
            host.tempCelsius?.let { context.getString(R.string.system_temp, it) },
            host.uptimeSeconds?.let { context.getString(R.string.system_uptime, duration(it)) },
        )
        return listOf(load, health).filter { it.isNotEmpty() }.joinToString("\n")
    }

    private fun guestsCard(hosts: SystemHosts): SystemRow.Card {
        val lines = when {
            !hosts.guestsAvailable -> context.getString(R.string.system_no_data_guests)
            else -> guestsLines(hosts.guests)
        }
        return SystemRow.Card(
            SystemSection.GUESTS,
            hosts.guestsLevel,
            context.getString(R.string.system_card_guests),
            lines,
        )
    }

    private fun guestsLines(guests: List<PveGuest>): String {
        val running = guests.count { it.running }
        val head = context.getString(R.string.system_guests_running, running, guests.size)
        val stopped = guests.filter { !it.running && it.onboot }.map { it.name }
        return when {
            stopped.isEmpty() -> head
            else -> head + "\n" + context.getString(R.string.system_guests_stopped, stopped.joinToString(", "))
        }
    }

    private fun clusterCard(cluster: KubeCluster): SystemRow.Card {
        val lines = when {
            !cluster.available -> context.getString(R.string.system_no_data_cluster)
            else -> clusterLines(cluster)
        }
        return SystemRow.Card(
            SystemSection.CLUSTER,
            cluster.level,
            context.getString(R.string.system_card_cluster),
            lines,
        )
    }

    private fun clusterLines(cluster: KubeCluster): String {
        val head = context.getString(R.string.system_cluster_pods, cluster.podCount, cluster.problemCount)
        val worst = cluster.namespaces.flatMap { it.pods }.firstOrNull { it.level != HealthLevel.OK }
        return when (worst) {
            null -> head
            else -> head + "\n" + join(worst.name, worst.reason.ifEmpty { worst.phase })
        }
    }

    private fun devicesCard(devices: SystemDevices): SystemRow.Card {
        val station = devices.station
        val stationLine = when {
            station == null -> context.getString(R.string.system_no_data_station)
            !station.up -> context.getString(R.string.system_station_down)
            else -> join(
                context.getString(R.string.system_station_short, station.version),
                station.latencyMs?.let { context.getString(R.string.system_latency, it) },
            )
        }
        val phones = when {
            !devices.phonesAvailable -> context.getString(R.string.system_phones_unavailable)
            else -> context.resources.getQuantityString(
                R.plurals.system_phone_count,
                devices.phones.size,
                devices.phones.size,
            )
        }
        return SystemRow.Card(
            SystemSection.DEVICES,
            devices.level,
            context.getString(R.string.system_card_devices),
            stationLine + "\n" + phones,
        )
    }

    private fun hostDetail(hosts: SystemHosts): List<SystemRow> {
        val host = hosts.host ?: return listOf(SystemRow.Note("host:none", context.getString(R.string.system_no_data_host)))
        return listOfNotNull(
            SystemRow.Item("host:state", hosts.hostLevel, host.name, stateText(host.up)),
            host.cpuRatio?.let { metric("host:cpu", R.string.system_cpu, ratioLevel(it), percentText(it)) },
            usageRow("host:mem", R.string.system_memory, host.memory),
            usageRow("host:disk", R.string.system_disk, host.disk),
            host.tempCelsius?.let { metric("host:temp", R.string.system_temperature, tempLevel(it), context.getString(R.string.system_temp, it)) },
            host.uptimeSeconds?.let { metric("host:uptime", R.string.system_uptime_title, HealthLevel.OK, duration(it)) },
        )
    }

    private fun metric(key: String, title: Int, level: HealthLevel, value: String) =
        SystemRow.Item(key, level, context.getString(title), value)

    private fun usageRow(key: String, title: Int, usage: Usage): SystemRow.Item? {
        val ratio = usage.ratio ?: return null
        return metric(key, title, ratioLevel(ratio), usageText(usage))
    }

    private fun guestsDetail(hosts: SystemHosts): List<SystemRow> = when {
        !hosts.guestsAvailable -> listOf(SystemRow.Note("guests:none", context.getString(R.string.system_no_data_guests)))
        hosts.guests.isEmpty() -> listOf(SystemRow.Note("guests:empty", context.getString(R.string.system_guests_empty)))
        else -> hosts.guests.map { guestRow(it) }
    }

    private fun guestRow(guest: PveGuest): SystemRow.Item {
        val kind = context.getString(if (guest.kind == GuestKind.CT) R.string.system_kind_ct else R.string.system_kind_vm)
        val title = context.getString(R.string.system_guest_title, guest.vmid, guest.name, kind)
        val subtitle = when {
            guest.running -> join(
                context.getString(R.string.system_running),
                guest.cpuRatio?.let { context.getString(R.string.system_cpu_short, percent(it)) },
                guest.memory.used?.let { context.getString(R.string.system_mem_value, usageText(guest.memory)) },
                guest.uptimeSeconds?.let { context.getString(R.string.system_uptime, duration(it)) },
            )
            guest.onboot -> join(context.getString(R.string.system_stopped), context.getString(R.string.system_onboot))
            else -> context.getString(R.string.system_stopped)
        }
        return SystemRow.Item("guest:${guest.vmid}", guest.level, title, subtitle)
    }

    private fun clusterDetail(cluster: KubeCluster): List<SystemRow> = when {
        !cluster.available -> listOf(SystemRow.Note("cluster:none", context.getString(R.string.system_no_data_cluster)))
        else -> cluster.namespaces.flatMap { namespaceRows(it) }
    }

    private fun namespaceRows(namespace: KubeNamespace): List<SystemRow> =
        listOf(SystemRow.Group("ns:${namespace.name}", namespace.level, namespace.name)) +
            namespace.pods.map { podRow(namespace.name, it) }

    private fun podRow(namespace: String, pod: KubePod): SystemRow.Item {
        val subtitle = join(
            pod.reason.ifEmpty { pod.phase },
            context.getString(R.string.system_pod_ready, pod.ready, pod.containers),
            context.getString(R.string.system_pod_restarts, pod.restarts),
            pod.cpuCores?.let { context.getString(R.string.system_pod_cpu, it * MILLICORES) },
            pod.memBytes?.let { bytes(it) },
            pod.ageSeconds?.let { context.getString(R.string.system_pod_age, duration(it)) },
        )
        return SystemRow.Item("pod:$namespace/${pod.name}", pod.level, pod.name, subtitle)
    }

    private fun devicesDetail(devices: SystemDevices): List<SystemRow> = buildList {
        val station = devices.station
        when (station) {
            null -> add(SystemRow.Note("station:none", context.getString(R.string.system_no_data_station)))
            else -> add(SystemRow.Item("station", station.level, station.model.ifEmpty { context.getString(R.string.system_station) }, stationText(devices)))
        }
        add(SystemRow.Group("phones", HealthLevel.OK, context.getString(R.string.system_phones)))
        addAll(phoneRows(devices))
    }

    private fun stationText(devices: SystemDevices): String {
        val station = checkNotNull(devices.station)
        return when {
            !station.up -> context.getString(R.string.system_station_down)
            else -> join(
                context.getString(R.string.system_pulsar_version, station.version),
                station.latencyMs?.let { context.getString(R.string.system_latency, it) },
                station.wifiRssiDbm?.let { context.getString(R.string.system_rssi, it) },
                station.tempCelsius?.let { context.getString(R.string.system_temp, it) },
                station.uptimeSeconds?.let { context.getString(R.string.system_uptime, duration(it)) },
            )
        }
    }

    private fun phoneRows(devices: SystemDevices): List<SystemRow> = when {
        !devices.phonesAvailable -> listOf(SystemRow.Note("phones:none", context.getString(R.string.system_phones_unavailable)))
        devices.phones.isEmpty() -> listOf(SystemRow.Note("phones:empty", context.getString(R.string.system_phones_empty)))
        else -> devices.phones.map { SystemRow.Item("phone:${it.id}", HealthLevel.OK, it.name.ifEmpty { it.id }, it.platform) }
    }

    private fun stateText(up: Boolean): String =
        context.getString(if (up) R.string.system_state_up else R.string.system_state_down)

    private fun usageText(usage: Usage): String {
        val used = usage.used ?: return ""
        val total = usage.total ?: return bytes(used)
        val ratio = usage.ratio ?: return bytes(used)
        return context.getString(R.string.system_usage, bytes(used), bytes(total), percent(ratio))
    }

    private fun percentText(ratio: Double): String = context.getString(R.string.system_percent, percent(ratio))

    private fun bytes(value: Double): String = Formatter.formatShortFileSize(context, value.toLong())

    private fun dateTime(unix: Long): String {
        val date = Date(unix * MILLIS)
        return DateFormat.getMediumDateFormat(context).format(date) + " " + DateFormat.getTimeFormat(context).format(date)
    }

    private fun duration(seconds: Double): String {
        val total = seconds.toLong()
        val days = total / DAY
        val hours = total % DAY / HOUR
        val minutes = total % HOUR / MINUTE
        return when {
            days > 0 -> context.getString(R.string.system_duration_days, days, hours)
            hours > 0 -> context.getString(R.string.system_duration_hours, hours, minutes)
            else -> context.getString(R.string.system_duration_minutes, minutes)
        }
    }

    private fun join(vararg parts: String?): String =
        parts.filterNot { it.isNullOrEmpty() }.joinToString(separator)

    private companion object {
        const val MILLIS = 1000L
        const val MINUTE = 60L
        const val HOUR = 3600L
        const val DAY = 86_400L
        const val MILLICORES = 1000.0
    }
}

internal fun percent(ratio: Double): Int = (ratio * 100).roundToInt()

internal fun ratioLevel(ratio: Double): HealthLevel = when {
    ratio >= 0.9 -> HealthLevel.WARNING
    else -> HealthLevel.OK
}

internal fun tempLevel(celsius: Double): HealthLevel = when {
    celsius >= 85.0 -> HealthLevel.CRITICAL
    celsius >= 75.0 -> HealthLevel.WARNING
    else -> HealthLevel.OK
}
