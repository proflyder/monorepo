package dev.proflyder.home.feature.vpn.presentation

import android.content.res.ColorStateList
import com.google.android.material.button.MaterialButton
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.impl.R

internal enum class VpnToggleLook {
    OFF,
    CONNECTING,
    ON,
}

internal object VpnToolbarToggle {
    fun look(connection: VpnConnection): VpnToggleLook = when (connection) {
        is VpnConnection.Connected -> VpnToggleLook.ON
        is VpnConnection.Connecting -> VpnToggleLook.CONNECTING
        else -> VpnToggleLook.OFF
    }

    fun render(button: MaterialButton, connection: VpnConnection) {
        val look = look(connection)
        val context = button.context
        button.setIconResource(icon(look))
        button.iconTint = ColorStateList.valueOf(MaterialColors.getColor(button, tint(look)))
        button.contentDescription = context.getString(description(look))
        button.tooltipText = button.contentDescription
    }

    private fun icon(look: VpnToggleLook): Int = when (look) {
        VpnToggleLook.OFF -> R.drawable.vpn_ic_shield_off
        VpnToggleLook.CONNECTING -> R.drawable.vpn_ic_shield_connecting
        VpnToggleLook.ON -> R.drawable.vpn_ic_shield
    }

    private fun tint(look: VpnToggleLook): Int = when (look) {
        VpnToggleLook.OFF -> com.google.android.material.R.attr.colorOnSurfaceVariant
        else -> androidx.appcompat.R.attr.colorPrimary
    }

    private fun description(look: VpnToggleLook): Int = when (look) {
        VpnToggleLook.OFF -> R.string.vpn_toggle_off
        VpnToggleLook.CONNECTING -> R.string.vpn_toggle_connecting
        VpnToggleLook.ON -> R.string.vpn_toggle_on
    }
}
