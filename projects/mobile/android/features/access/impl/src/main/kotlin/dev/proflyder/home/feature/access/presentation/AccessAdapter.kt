package dev.proflyder.home.feature.access.presentation

import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.core.view.isVisible
import dev.proflyder.home.core.model.AccessToken
import dev.proflyder.home.core.presentation.KeyedDiff
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.feature.access.impl.databinding.ItemAccessTokenBinding

internal class AccessAdapterCallback(
    private val onOpen: (AccessToken) -> Unit,
    val menu: (View, AccessToken) -> Unit,
) : BaseAdapterCallback<AccessToken>() {
    override fun onItemClick(item: AccessToken) = onOpen(item)
}

internal class AccessAdapter(
    callback: AccessAdapterCallback,
) : BaseListAdapter<AccessToken, ItemAccessTokenBinding, AccessAdapterCallback>(
    callback,
    KeyedDiff(key = AccessToken::id),
) {
    override fun onCreateHolder(
        inflater: LayoutInflater,
        parent: ViewGroup,
        viewType: Int,
    ): BaseViewHolder<AccessToken, ItemAccessTokenBinding, AccessAdapterCallback> =
        Holder(ItemAccessTokenBinding.inflate(inflater, parent, false), callback)

    private class Holder(
        binding: ItemAccessTokenBinding,
        callback: AccessAdapterCallback,
    ) : BaseViewHolder<AccessToken, ItemAccessTokenBinding, AccessAdapterCallback>(binding, callback) {
        init {
            forwardRowClicks()
            binding.accessItemMenu.setOnClickListener { callback.menu(binding.accessItemMenu, item) }
        }

        override fun bind(item: AccessToken) {
            val context = binding.root.context
            binding.accessItemName.text = item.name
            binding.accessItemOwner.isVisible = item.owner
            binding.accessItemModules.text = item.summary(context)
            binding.accessItemMenu.isVisible = !item.owner
        }
    }
}
