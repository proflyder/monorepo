package dev.proflyder.home.feature.control.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import dev.proflyder.home.core.designsystem.list.SectionedAdapter
import dev.proflyder.home.core.presentation.IntentAdapterCallback
import dev.proflyder.home.core.presentation.KeyedDiff
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.ui.ViewBinding
import dev.proflyder.home.feature.control.ControlItem
import dev.proflyder.home.feature.control.impl.databinding.ItemControlDeviceBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlDeviceGroupBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlFiltersBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlHeaderBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlHubBinding
import dev.proflyder.home.feature.control.impl.databinding.ItemControlScenarioBinding

internal class ControlAdapterCallback(
    val dispatch: (ControlIntent) -> Unit,
) : AdapterCallback<ControlItem> by IntentAdapterCallback(
    dispatch = dispatch,
    onClick = { item -> ControlIntent.Tap(TapTarget.Item(item)) },
    onLongClick = { item -> ControlIntent.Hold(TapTarget.Item(item)) },
)

internal typealias ControlHolder = BaseViewHolder<ControlItem, ViewBinding, ControlAdapterCallback>

internal val ControlDiff = KeyedDiff<ControlItem>(key = { it.key })

internal class ControlAdapter(
    callback: ControlAdapterCallback,
) : BaseListAdapter<ControlItem, ViewBinding, ControlAdapterCallback>(callback, ControlDiff),
    SectionedAdapter {
    override fun isSectionHeader(position: Int): Boolean = getItem(position) is ControlItem.Header

    override fun viewTypeOf(item: ControlItem): Int = ControlViewType.of(item).ordinal

    override fun onCreateHolder(inflater: LayoutInflater, parent: ViewGroup, viewType: Int): ControlHolder =
        when (ControlViewType.entries[viewType]) {
            ControlViewType.HEADER -> HeaderHolder(ItemControlHeaderBinding.inflate(inflater, parent, false), callback)
            ControlViewType.SCENARIO -> ScenarioHolder(ItemControlScenarioBinding.inflate(inflater, parent, false), callback)
            ControlViewType.DEVICE -> DeviceHolder(ItemControlDeviceBinding.inflate(inflater, parent, false), callback)
            ControlViewType.GROUP -> DeviceGroupHolder(ItemControlDeviceGroupBinding.inflate(inflater, parent, false), callback)
            ControlViewType.HUB -> HubHolder(ItemControlHubBinding.inflate(inflater, parent, false), callback)
            ControlViewType.FILTERS -> FiltersHolder(ItemControlFiltersBinding.inflate(inflater, parent, false), callback)
        }
}

internal enum class ControlViewType {
    HEADER,
    SCENARIO,
    DEVICE,
    GROUP,
    HUB,
    FILTERS,
    ;

    companion object {
        fun of(item: ControlItem): ControlViewType = when (item) {
            is ControlItem.Header -> HEADER
            is ControlItem.Scenario -> SCENARIO
            is ControlItem.Device -> DEVICE
            is ControlItem.DeviceGroup -> GROUP
            is ControlItem.Hub -> HUB
            is ControlItem.Filters -> FILTERS
        }
    }
}
