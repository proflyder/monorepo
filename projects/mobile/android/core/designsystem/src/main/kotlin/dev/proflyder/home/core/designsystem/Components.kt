package dev.proflyder.home.core.designsystem

import android.view.LayoutInflater
import android.view.ViewGroup
import com.google.android.material.button.MaterialButton
import com.google.android.material.chip.Chip

object Components {
    fun filterChip(parent: ViewGroup): Chip = inflate(parent, R.layout.ds_chip_filter)

    fun inputChip(parent: ViewGroup): Chip = inflate(parent, R.layout.ds_chip_input)

    fun tonalButtonL(parent: ViewGroup): MaterialButton = inflate(parent, R.layout.ds_button_tonal_l)

    @Suppress("UNCHECKED_CAST")
    private fun <T> inflate(parent: ViewGroup, layout: Int): T = LayoutInflater.from(parent.context).inflate(layout, parent, false) as T
}
