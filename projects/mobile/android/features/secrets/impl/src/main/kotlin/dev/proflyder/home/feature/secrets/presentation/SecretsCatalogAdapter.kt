package dev.proflyder.home.feature.secrets.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import dev.proflyder.home.core.designsystem.list.SectionedAdapter
import dev.proflyder.home.core.presentation.KeyedDiff
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.ui.ViewBinding
import dev.proflyder.home.feature.secrets.impl.databinding.ItemSecretDeviceCardBinding
import dev.proflyder.home.feature.secrets.impl.databinding.ItemSecretGroupBinding
import dev.proflyder.home.feature.secrets.impl.databinding.ItemSecretPathBinding

private typealias CatalogCallback = AdapterCallback<SecretsCatalogRow>

private typealias CatalogHolder = BaseViewHolder<SecretsCatalogRow, ViewBinding, CatalogCallback>

private val CatalogDiff = KeyedDiff<SecretsCatalogRow>(key = { it.key })

internal class SecretsCatalogAdapter(
    private val onClick: (SecretsCatalogRow.EntryRow) -> Unit,
    private val onBindDevice: () -> Unit = {},
) : BaseListAdapter<SecretsCatalogRow, ViewBinding, CatalogCallback>(BaseAdapterCallback<SecretsCatalogRow>(), CatalogDiff),
    SectionedAdapter {
    override fun isSectionHeader(position: Int): Boolean = getItem(position) !is SecretsCatalogRow.EntryRow

    override fun isStickyHeader(position: Int): Boolean = getItem(position) is SecretsCatalogRow.Group

    override fun viewTypeOf(item: SecretsCatalogRow): Int = when (item) {
        SecretsCatalogRow.DeviceCard -> TYPE_DEVICE
        is SecretsCatalogRow.Group -> TYPE_GROUP
        is SecretsCatalogRow.EntryRow -> TYPE_ENTRY
    }

    override fun onCreateHolder(inflater: LayoutInflater, parent: ViewGroup, viewType: Int): CatalogHolder =
        when (viewType) {
            TYPE_DEVICE -> DeviceHolder(ItemSecretDeviceCardBinding.inflate(inflater, parent, false))
            TYPE_GROUP -> GroupHolder(ItemSecretGroupBinding.inflate(inflater, parent, false))
            else -> EntryHolder(ItemSecretPathBinding.inflate(inflater, parent, false))
        }

    private inner class DeviceHolder(private val views: ItemSecretDeviceCardBinding) :
        CatalogHolder(views, BaseAdapterCallback<SecretsCatalogRow>()) {
        override fun bind(item: SecretsCatalogRow) {
            views.secretDeviceCardBind.setOnClickListener { onBindDevice() }
        }
    }

    private class GroupHolder(private val views: ItemSecretGroupBinding) : CatalogHolder(views, BaseAdapterCallback<SecretsCatalogRow>()) {
        override fun bind(item: SecretsCatalogRow) {
            views.secretGroupTitle.text = (item as SecretsCatalogRow.Group).name
        }
    }

    private inner class EntryHolder(private val views: ItemSecretPathBinding) : CatalogHolder(views, BaseAdapterCallback<SecretsCatalogRow>()) {
        override fun bind(item: SecretsCatalogRow) {
            val row = item as SecretsCatalogRow.EntryRow
            views.secretPathRow.title = row.entry.path
            views.secretPathRow.subtitle = row.entry.description.ifEmpty { row.entry.consumer.orEmpty() }
            views.secretPathRow.chevron = true
            views.root.setOnClickListener { onClick(row) }
        }
    }

    private companion object {
        const val TYPE_DEVICE = 0
        const val TYPE_GROUP = 1
        const val TYPE_ENTRY = 2
    }
}
