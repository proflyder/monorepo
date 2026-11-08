package dev.proflyder.home.feature.control.presentation

import android.content.Context
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.ui.serverText

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
