package dev.proflyder.home.feature.control.presentation

import android.content.Context
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.ui.serverText
import dev.proflyder.home.feature.control.impl.R

internal fun ControlEntry.localizedTitle(context: Context): String =
    serverText(context, titleKey, titleArgs) ?: title

internal fun ControlEntry.localizedDisplayName(context: Context): String {
    val name = localizedTitle(context).trim()
    val technical = title.isBlank() || title.trim() == id
    return if (device.isNotBlank() && technical) device.trim() else name.ifEmpty { device.trim() }
}

internal fun ControlEntry.localizedDetail(context: Context): String? =
    serverText(context, detailKey, emptyMap())?.takeUnless { it.isBlank() }

internal fun ControlEntry.localizedVolumeText(context: Context, unavailable: Int): String {
    val volume = volume ?: return localizedStateText(context, unavailable)
    if (!available) return context.getString(unavailable)
    val main = when {
        volume.muted -> context.getString(R.string.control_volume_muted)
        else -> context.getString(R.string.control_volume_percent, volume.level)
    }
    val announcement = volumeCompanion?.volume?.level ?: return main
    return context.getString(R.string.control_volume_with_announcement, main, announcement)
}

internal fun ControlEntry.localizedStateText(context: Context, unavailable: Int): String {
    val state = when {
        !available -> context.getString(unavailable)
        power == PowerState.ON -> context.getString(R.string.control_state_on)
        power == PowerState.OFF -> context.getString(R.string.control_state_off)
        else -> context.getString(R.string.control_state_unknown)
    }
    val detail = localizedDetail(context) ?: return state
    return context.getString(R.string.control_state_detail, state, detail)
}
