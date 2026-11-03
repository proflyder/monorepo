package dev.proflyder.home.feature.control.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.DiffUtil
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.ui.ViewBinding
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.ItemControlDeviceBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlHeaderBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlProfileBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlScenarioBinding

/** Row taps come from [AdapterCallback]; the switch reports an explicit target state. */
internal interface ControlAdapterCallback : AdapterCallback<ControlItem> {
    fun onPowerChanged(entry: ControlEntry, on: Boolean)

    /** The profile selector reports the chosen side: `work = true` is the named profile. */
    fun onProfileChanged(entry: ControlEntry, work: Boolean)
}

private object ControlDiff : DiffUtil.ItemCallback<ControlItem>() {
    override fun areItemsTheSame(oldItem: ControlItem, newItem: ControlItem): Boolean = oldItem.key == newItem.key
    override fun areContentsTheSame(oldItem: ControlItem, newItem: ControlItem): Boolean = oldItem == newItem
}

private typealias ControlHolder = BaseViewHolder<ControlItem, ViewBinding, ControlAdapterCallback>

internal class ControlAdapter(
    callback: ControlAdapterCallback,
) : BaseListAdapter<ControlItem, ViewBinding, ControlAdapterCallback>(callback, ControlDiff) {

    override fun viewTypeOf(item: ControlItem): Int = when (item) {
        is ControlItem.Header -> TYPE_HEADER
        is ControlItem.Profile -> TYPE_PROFILE
        is ControlItem.Scenario -> TYPE_SCENARIO
        is ControlItem.Device -> TYPE_DEVICE
    }

    override fun onCreateHolder(inflater: LayoutInflater, parent: ViewGroup, viewType: Int): ControlHolder =
        when (viewType) {
            TYPE_HEADER -> HeaderHolder(ItemControlHeaderBinding.inflate(inflater, parent, false), callback)
            TYPE_PROFILE -> ProfileHolder(ItemControlProfileBinding.inflate(inflater, parent, false), callback)
            TYPE_SCENARIO -> ScenarioHolder(ItemControlScenarioBinding.inflate(inflater, parent, false), callback)
            else -> DeviceHolder(ItemControlDeviceBinding.inflate(inflater, parent, false), callback)
        }

    private class HeaderHolder(
        private val views: ItemControlHeaderBinding,
        callback: ControlAdapterCallback,
    ) : ControlHolder(views, callback) {
        override fun bind(item: ControlItem) {
            val header = item as ControlItem.Header
            views.controlHeaderTitle.text = header.title
            views.controlHeaderSummary.text = header.summary.orEmpty()
            views.controlHeaderSummary.setVisible(header.summary != null)
        }
    }

    /**
     * The profile is a mode, not a device: two segmented buttons instead of a
     * switch. The card itself is not clickable — only an explicit side acts,
     * because "toggle" is not a safe action for an at-least-once delivery.
     */
    private class ProfileHolder(
        private val views: ItemControlProfileBinding,
        callback: ControlAdapterCallback,
    ) : ControlHolder(views, callback) {
        /** Programmatic `check` must not be reported as a user choice. */
        private var binding_ = false

        init {
            views.controlProfilePersonal.setText(R.string.control_profile_personal)
            views.controlProfileWork.setText(R.string.control_profile_work)
            views.controlProfileGroup.addOnButtonCheckedListener { _, checkedId, isChecked ->
                if (binding_ || !isChecked) return@addOnButtonCheckedListener
                val row = item as ControlItem.Profile
                callback.onProfileChanged(row.entry, checkedId == views.controlProfileWork.id)
            }
        }

        override fun bind(item: ControlItem) {
            val row = item as ControlItem.Profile
            val entry = row.entry
            val work = entry.power == PowerState.ON
            views.controlProfileTitle.text = entry.title
            views.controlProfileState.setText(
                when {
                    !entry.available -> R.string.control_state_unavailable
                    entry.power == null -> R.string.control_profile_states_unknown
                    work -> R.string.control_profile_work
                    else -> R.string.control_profile_personal
                },
            )
            val tint = MaterialColors.getColor(
                views.root,
                if (work) com.google.android.material.R.attr.colorOnPrimaryContainer
                else com.google.android.material.R.attr.colorOnSurfaceVariant,
            )
            views.controlProfileIcon.setImageResource(ControlIcons.resolve(entry))
            views.controlProfileIcon.setColorFilter(tint)
            views.controlProfileIcon.isActivated = work
            views.controlProfileProgress.setVisible(row.busy)
            val enabled = entry.available && !row.busy
            views.root.alpha = if (entry.available) 1f else DISABLED_ALPHA
            views.controlProfileGroup.isEnabled = enabled
            views.controlProfilePersonal.isEnabled = enabled
            views.controlProfileWork.isEnabled = enabled
            binding_ = true
            when (entry.power) {
                PowerState.ON -> views.controlProfileGroup.check(views.controlProfileWork.id)
                PowerState.OFF -> views.controlProfileGroup.check(views.controlProfilePersonal.id)
                // Neither side is claimed while the server state is unknown.
                null -> views.controlProfileGroup.clearChecked()
            }
            binding_ = false
        }
    }

    private class ScenarioHolder(
        private val views: ItemControlScenarioBinding,
        callback: ControlAdapterCallback,
    ) : ControlHolder(views, callback) {
        init {
            forwardRowClicks()
        }

        override fun bind(item: ControlItem) {
            val row = item as ControlItem.Scenario
            views.controlScenarioTitle.text = row.entry.title
            views.controlScenarioIcon.setImageResource(ControlIcons.resolve(row.entry))
            views.controlScenarioProgress.setVisible(row.busy)
            views.root.alpha = if (row.entry.available) 1f else DISABLED_ALPHA
            views.root.isEnabled = row.entry.available && !row.busy
        }
    }

    private class DeviceHolder(
        private val views: ItemControlDeviceBinding,
        callback: ControlAdapterCallback,
    ) : ControlHolder(views, callback) {
        /** Programmatic `isChecked` must not be reported as a user toggle. */
        private var binding_ = false

        init {
            forwardRowClicks()
            views.controlDeviceSwitch.setOnCheckedChangeListener { _, checked ->
                if (!binding_) {
                    val row = item as ControlItem.Device
                    callback.onPowerChanged(row.entry, checked)
                }
            }
        }

        override fun bind(item: ControlItem) {
            val row = item as ControlItem.Device
            val entry = row.entry
            val on = entry.power == PowerState.ON
            views.controlDeviceTitle.text = entry.title
            views.controlDeviceState.setText(
                when {
                    !entry.available -> R.string.control_state_unavailable
                    entry.power == PowerState.ON -> R.string.control_state_on
                    entry.power == PowerState.OFF -> R.string.control_state_off
                    else -> R.string.control_state_unknown
                },
            )
            val brightness = entry.light?.takeIf { on && entry.available && it.brightness && it.brightnessPct > 0 }?.brightnessPct
            if (brightness != null) {
                views.controlDeviceState.text = views.root.context.getString(R.string.control_state_on_brightness, brightness)
            }
            views.controlDeviceIcon.setImageResource(ControlIcons.resolve(entry))
            val tint = MaterialColors.getColor(
                views.root,
                if (on) com.google.android.material.R.attr.colorOnPrimaryContainer
                else com.google.android.material.R.attr.colorOnSurfaceVariant,
            )
            views.controlDeviceIcon.setColorFilter(tint)
            views.controlDeviceIcon.isActivated = on
            views.controlDeviceProgress.setVisible(row.busy)
            views.root.alpha = if (entry.available) 1f else DISABLED_ALPHA
            views.controlDeviceSwitch.isEnabled = entry.available && !row.busy
            views.root.isEnabled = entry.available && !row.busy
            binding_ = true
            views.controlDeviceSwitch.isChecked = on
            binding_ = false
        }
    }

    private companion object {
        const val TYPE_HEADER = 0
        const val TYPE_SCENARIO = 1
        const val TYPE_DEVICE = 2
        const val TYPE_PROFILE = 3
        const val DISABLED_ALPHA = 0.5f
    }
}
