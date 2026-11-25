package dev.proflyder.home.feature.hermes.presentation

import dev.proflyder.home.core.designsystem.Tone
import dev.proflyder.home.core.navigation.Destination

internal sealed interface HermesRow {
    val key: String

    data class Header(override val key: String, val title: String) : HermesRow

    data class Entry(
        override val key: String,
        val title: String,
        val subtitle: String = "",
        val value: String = "",
        val tone: Tone = Tone.NEUTRAL,
        val target: Destination? = null,
    ) : HermesRow

    data class Note(override val key: String, val text: String) : HermesRow

    data class Body(override val key: String, val text: String) : HermesRow
}
