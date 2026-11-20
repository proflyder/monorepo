package dev.proflyder.home.feature.control.presentation

import androidx.annotation.DrawableRes
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.feature.control.impl.R

internal object ControlIcons {
    private val keywords: List<Pair<List<String>, Int>> = listOf(
        listOf("night", "moon", "sleep", "bed") to R.drawable.control_ic_moon,
        listOf("sun", "weather-sunny", "day", "morning") to R.drawable.control_ic_sun,
        listOf("laptop", "desk", "monitor", "briefcase", "work", "computer") to R.drawable.control_ic_work,
        listOf("bulb", "lamp", "light", "ceiling", "led") to R.drawable.control_ic_bulb,
        listOf("lock-open", "lock_open", "unlock", "unlocked") to R.drawable.control_ic_lock_open,
        listOf("lock", "locked", "key") to R.drawable.control_ic_lock,
        listOf("plug", "power", "outlet", "socket", "fan") to R.drawable.control_ic_plug,
        listOf("play", "movie", "television", "tv", "theater", "scene") to R.drawable.control_ic_play,
        listOf("message", "speak", "voice", "text", "comment") to R.drawable.control_ic_message,
        listOf("volume", "speaker", "bullhorn", "megaphone") to R.drawable.control_ic_volume_up,
    )

    @DrawableRes
    fun resolve(entry: ControlEntry): Int {
        if (entry.kind == ControlKind.PROFILE) return R.drawable.control_ic_account
        val hint = entry.icon.lowercase()
        if (hint.isNotEmpty()) {
            keywords.firstOrNull { (words, _) -> words.any { it in hint } }?.let { return it.second }
        }
        return when (entry.kind) {
            ControlKind.LIGHT -> R.drawable.control_ic_bulb
            ControlKind.SWITCH -> R.drawable.control_ic_plug
            ControlKind.BUTTON -> R.drawable.control_ic_lock
            ControlKind.PROFILE -> R.drawable.control_ic_account
            else -> R.drawable.control_ic_play
        }
    }
}
