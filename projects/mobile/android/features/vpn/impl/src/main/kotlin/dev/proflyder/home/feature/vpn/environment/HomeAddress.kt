package dev.proflyder.home.feature.vpn.environment

import java.net.Inet4Address
import java.net.Inet6Address
import java.net.InetAddress

internal object HomeAddress {
    fun isPrivate(address: InetAddress): Boolean = when (address) {
        is Inet4Address -> address.isSiteLocalAddress
        is Inet6Address -> isUniqueLocal(address) || address.isSiteLocalAddress
        else -> false
    }

    private fun isUniqueLocal(address: Inet6Address): Boolean =
        address.address[0].toInt() and ULA_MASK == ULA_PREFIX

    private const val ULA_MASK = 0xFE
    private const val ULA_PREFIX = 0xFC
}
