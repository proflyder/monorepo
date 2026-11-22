package dev.proflyder.home.feature.vpn

import dev.proflyder.home.core.model.LocalJson
import dev.proflyder.home.core.model.TransportSettings
import dev.proflyder.home.core.model.VpnProfile
import dev.proflyder.home.core.model.VpnProtocolId
import kotlinx.serialization.json.JsonObject

internal object VpnSamples {
    private val wireGuardJson = """
        {"interface":{"address":"10.77.2.2/32","dns":["192.0.2.1"],"mtu":1380},
         "peer":{"publicKey":"server-key","endpoint":"vpn.example.com:51820","lanEndpoint":"192.0.2.29:51820",
                 "allowedIps":["0.0.0.0/0"],"persistentKeepalive":25},
         "splitAllowedIps":["192.0.2.0/24"]}
    """.trimIndent()

    private val ocservJson = """
        {"url":"https://vpn.example.com","group":"full","splitGroup":"lan","authentication":["certificate","password+totp"]}
    """.trimIndent()

    fun settings(json: String): TransportSettings = TransportSettings(LocalJson.decodeFromString(JsonObject.serializer(), json))

    val wireGuard: TransportSettings = settings(wireGuardJson)

    val ocserv: TransportSettings = settings(ocservJson)

    val profile = VpnProfile(
        deviceId = "device-1",
        deviceName = "Pixel",
        transports = mapOf(VpnProtocolId("wireguard") to wireGuard, VpnProtocolId("ocserv") to ocserv),
        createdUnix = 1_795_100_000,
        updatedUnix = 1_795_100_100,
    )
}
