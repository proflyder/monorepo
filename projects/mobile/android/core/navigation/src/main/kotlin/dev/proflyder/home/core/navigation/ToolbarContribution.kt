package dev.proflyder.home.core.navigation

import android.view.ViewGroup
import androidx.annotation.DrawableRes
import androidx.annotation.IdRes
import androidx.annotation.StringRes
import androidx.appcompat.app.AppCompatActivity

class ToolbarSlots(
    val start: ViewGroup,
    val end: ViewGroup,
    val below: ViewGroup,
)

interface ToolbarContribution {
    val order: Int

    fun attach(activity: AppCompatActivity, slots: ToolbarSlots, navigator: Navigator)
}

interface ToolbarAction {
    @get:IdRes
    val itemId: Int

    val order: Int

    @get:StringRes
    val titleRes: Int

    @get:DrawableRes
    val iconRes: Int

    fun perform(navigator: Navigator)
}
