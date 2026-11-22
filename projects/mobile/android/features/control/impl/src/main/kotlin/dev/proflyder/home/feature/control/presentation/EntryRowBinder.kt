package dev.proflyder.home.feature.control.presentation

import android.content.Context
import android.view.View
import android.widget.CompoundButton
import android.widget.ImageView
import android.widget.TextView
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.designsystem.DeviceStateViews
import dev.proflyder.home.core.designsystem.bindAvailability
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.presentation.SwitchBinder
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.control.impl.R

internal class EntryRowViews(
    val root: View,
    val icon: ImageView,
    val state: TextView,
    val switch: CompoundButton,
    val progress: View,
    val card: (Boolean) -> Unit = {},
)

internal data class EntryRow(
    val entry: ControlEntry,
    val busy: Boolean,
    val tap: ControlIntent,
    val hold: ControlIntent,
)

internal class EntryRowBinder(
    private val views: EntryRowViews,
    private val dispatch: (ControlIntent) -> Unit,
) {
    private var row: EntryRow? = null
    private val power = SwitchBinder(views.switch) { on -> row?.let { dispatch(powerIntent(it.entry, on)) } }

    init {
        views.root.setOnClickListener { row?.let { tap(it) } }
        views.root.setOnLongClickListener { row?.let { dispatch(it.hold) } != null }
    }

    fun bind(row: EntryRow) {
        this.row = row
        val entry = row.entry
        views.icon.setImageResource(ControlIcons.resolve(entry))
        views.progress.setVisible(row.busy)
        views.root.bindAvailability(entry.available)
        views.root.isEnabled = entry.enabledWhen(row.busy)
        when {
            entry.isVolume -> bindVolume(entry)
            entry.isButton -> bindButton()
            else -> bindPower(row)
        }
    }

    private fun tap(row: EntryRow) {
        if (row.entry.isButton && !row.entry.enabledWhen(row.busy)) return
        dispatch(row.tap)
    }

    private fun bindVolume(entry: ControlEntry) {
        views.switch.setVisible(false)
        views.state.setVisible(true)
        views.state.text = entry.localizedVolumeText(views.root.context, R.string.control_state_unavailable)
        val active = entry.available && entry.volume?.muted != true
        DeviceStateViews.bindIcon(views.icon, active)
        DeviceStateViews.bindStatus(views.state, active)
        views.card(false)
    }

    private fun bindButton() {
        views.switch.setVisible(false)
        views.state.setVisible(false)
        DeviceStateViews.clearIcon(views.icon)
        views.icon.setColorFilter(
            MaterialColors.getColor(views.root, com.google.android.material.R.attr.colorOnPrimaryContainer),
        )
        views.icon.isActivated = true
        views.card(false)
    }

    private fun bindPower(row: EntryRow) {
        val entry = row.entry
        val on = entry.isOn
        views.switch.setVisible(true)
        views.state.setVisible(true)
        DeviceStateViews.bindIcon(views.icon, on)
        views.card(on)
        power.render(on)
        views.switch.isEnabled = entry.enabledWhen(row.busy)
        views.state.text = entry.powerStateText(views.root.context)
        DeviceStateViews.bindStatus(views.state, on && entry.available)
    }

    private fun powerIntent(entry: ControlEntry, on: Boolean): ControlIntent =
        ControlIntent.Command(DeviceCommand.Power(entry, on))
}

internal fun ControlEntry.enabledWhen(busy: Boolean): Boolean = available && !busy

internal fun ControlEntry.powerStateText(context: Context): String {
    val brightness = light?.takeIf { isOn && available && it.brightness && it.brightnessPct > 0 }?.brightnessPct
    return when (brightness) {
        null -> localizedStateText(context, R.string.control_state_unavailable)
        else -> context.getString(R.string.control_state_on_brightness, brightness)
    }
}
