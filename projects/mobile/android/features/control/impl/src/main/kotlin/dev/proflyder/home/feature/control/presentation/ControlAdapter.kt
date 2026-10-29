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
import dev.proflyder.home.feature.control.impl.databinding.ItemControlScenarioBinding

/** Row taps come from [AdapterCallback]; the switch reports an explicit target state. */
internal interface ControlAdapterCallback : AdapterCallback<ControlItem> {
    fun onPowerChanged(entry: ControlEntry, on: Boolean)
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
        is ControlItem.Scenario -> TYPE_SCENARIO
        is ControlItem.Device -> TYPE_DEVICE
    }

    override fun onCreateHolder(inflater: LayoutInflater, parent: ViewGroup, viewType: Int): ControlHolder =
        when (viewType) {
            TYPE_HEADER -> HeaderHolder(ItemControlHeaderBinding.inflate(inflater, parent, false), callback)
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
        const val DISABLED_ALPHA = 0.5f
    }
}
