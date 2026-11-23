package dev.proflyder.home.feature.vpn.transport

import kotlinx.coroutines.flow.Flow

internal interface SystemVpnStarts {
    val starts: Flow<Unit>
}
