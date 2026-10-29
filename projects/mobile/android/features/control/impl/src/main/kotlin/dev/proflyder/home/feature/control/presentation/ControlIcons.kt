package dev.proflyder.home.feature.control.presentation

import androidx.annotation.DrawableRes
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.feature.control.impl.R

/**
 * Maps the icon hint from Home Assistant (an mdi name without the prefix, e.g.
 * "lightbulb-outline", "weather-night", "laptop") to bundled artwork by keyword,
 * falling back by kind. Nothing here lists entities: only generic keywords.
 */
internal object ControlIcons {
    private val keywords: List<Pair<List<String>, Int>> = listOf(
        listOf("night", "moon", "sleep", "bed") to R.drawable.control_ic_moon,
        listOf("sun", "weather-sunny", "day", "morning") to R.drawable.control_ic_sun,
        listOf("laptop", "desk", "monitor", "briefcase", "work", "computer") to R.drawable.control_ic_work,
        listOf("bulb", "lamp", "light", "ceiling", "led") to R.drawable.control_ic_bulb,
        listOf("plug", "power", "outlet", "socket", "fan") to R.drawable.control_ic_plug,
        listOf("play", "movie", "television", "tv", "theater", "scene") to R.drawable.control_ic_play,
    )

    @DrawableRes
    fun resolve(entry: ControlEntry): Int {
        val hint = entry.icon.lowercase()
        if (hint.isNotEmpty()) {
            keywords.firstOrNull { (words, _) -> words.any { it in hint } }?.let { return it.second }
        }
        return when (entry.kind) {
            ControlKind.LIGHT -> R.drawable.control_ic_bulb
            ControlKind.SWITCH -> R.drawable.control_ic_plug
            else -> R.drawable.control_ic_play
        }
    }
}
