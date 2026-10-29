package dev.proflyder.home.feature.control.presentation

import androidx.annotation.DrawableRes
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.feature.control.impl.R

/** Maps the server's free-form icon hint to bundled artwork, falling back by kind. */
internal object ControlIcons {
    @DrawableRes
    fun resolve(entry: ControlEntry): Int = when (entry.icon) {
        "work", "desk", "laptop" -> R.drawable.control_ic_work
        "moon", "night", "sleep" -> R.drawable.control_ic_moon
        "sun", "day", "morning" -> R.drawable.control_ic_sun
        "bulb", "light", "lamp" -> R.drawable.control_ic_bulb
        "plug", "power", "outlet" -> R.drawable.control_ic_plug
        "play", "movie", "scene" -> R.drawable.control_ic_play
        else -> when (entry.kind) {
            ControlKind.LIGHT -> R.drawable.control_ic_bulb
            ControlKind.SWITCH -> R.drawable.control_ic_plug
            else -> R.drawable.control_ic_play
        }
    }
}
