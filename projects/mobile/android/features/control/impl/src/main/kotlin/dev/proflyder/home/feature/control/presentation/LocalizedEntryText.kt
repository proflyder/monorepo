package dev.proflyder.home.feature.control.presentation

import android.content.Context
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.ui.serverText
import dev.proflyder.home.feature.control.impl.R

/** The entry title with a server localization key applied, or the raw title. */
internal fun ControlEntry.localizedTitle(context: Context): String =
    serverText(context, titleKey, titleArgs) ?: title

/**
 * [ControlEntry.displayName] with the localized title substituted. Home
 * Assistant user data has no key, so it is returned unchanged.
 */
internal fun ControlEntry.localizedDisplayName(context: Context): String {
    val name = localizedTitle(context).trim()
    val technical = title.isBlank() || title.trim() == id
    return if (device.isNotBlank() && technical) device.trim() else name.ifEmpty { device.trim() }
}

/** The server-provided device status localized locally, or null when absent. */
internal fun ControlEntry.localizedDetail(context: Context): String? =
    serverText(context, detailKey, emptyMap())?.takeUnless { it.isBlank() }

/**
 * The subtitle of a device row: the on/off text (or the unavailable text)
 * followed by an optional device detail such as "· заблокирован".
 */
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
