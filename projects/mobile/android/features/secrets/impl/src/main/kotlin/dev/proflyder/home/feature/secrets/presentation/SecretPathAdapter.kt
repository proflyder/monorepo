package dev.proflyder.home.feature.secrets.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import dev.proflyder.home.core.designsystem.joinMeta
import dev.proflyder.home.core.designsystem.list.SectionedAdapter
import dev.proflyder.home.core.designsystem.list.applyFullRowRipple
import dev.proflyder.home.core.model.SecretVersion
import dev.proflyder.home.core.presentation.KeyedDiff
import dev.proflyder.home.core.ui.AdapterCallback
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.ui.ViewBinding
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.secrets.impl.R
import dev.proflyder.home.feature.secrets.impl.databinding.ItemSecretHeaderBinding
import dev.proflyder.home.feature.secrets.impl.databinding.ItemSecretKeyBinding
import dev.proflyder.home.feature.secrets.impl.databinding.ItemSecretNoteBinding
import dev.proflyder.home.feature.secrets.impl.databinding.ItemSecretValueBinding
import dev.proflyder.home.feature.secrets.impl.databinding.ItemSecretVersionBinding

internal interface SecretPathActions {
    fun reveal(key: String)

    fun copy(key: String)

    fun edit(key: String)

    fun rollback(version: Long)

    fun delete()
}

private typealias PathCallback = AdapterCallback<SecretPathRow>

private typealias PathHolder = BaseViewHolder<SecretPathRow, ViewBinding, PathCallback>

private val PathDiff = KeyedDiff<SecretPathRow>(key = { it.key })

internal class SecretPathAdapter(
    private val actions: SecretPathActions,
) : BaseListAdapter<SecretPathRow, ViewBinding, PathCallback>(BaseAdapterCallback(), PathDiff),
    SectionedAdapter {
    override fun isSectionHeader(position: Int): Boolean = getItem(position).let {
        it !is SecretPathRow.KeyRow && it !is SecretPathRow.VersionRow && it !is SecretPathRow.ValueBody
    }

    override fun viewTypeOf(item: SecretPathRow): Int = when (item) {
        is SecretPathRow.Header -> TYPE_HEADER
        is SecretPathRow.KeyRow -> TYPE_KEY
        is SecretPathRow.VersionRow -> TYPE_VERSION
        is SecretPathRow.Note -> TYPE_NOTE
        is SecretPathRow.ValueBody -> TYPE_VALUE
    }

    override fun onCreateHolder(inflater: LayoutInflater, parent: ViewGroup, viewType: Int): PathHolder =
        when (viewType) {
            TYPE_HEADER -> HeaderHolder(ItemSecretHeaderBinding.inflate(inflater, parent, false))
            TYPE_KEY -> KeyHolder(ItemSecretKeyBinding.inflate(inflater, parent, false))
            TYPE_VERSION -> VersionHolder(ItemSecretVersionBinding.inflate(inflater, parent, false))
            TYPE_VALUE -> ValueHolder(ItemSecretValueBinding.inflate(inflater, parent, false))
            else -> NoteHolder(ItemSecretNoteBinding.inflate(inflater, parent, false))
        }

    private inner class HeaderHolder(
        private val views: ItemSecretHeaderBinding,
    ) : PathHolder(views, BaseAdapterCallback()) {
        override fun bind(item: SecretPathRow) {
            val header = item as SecretPathRow.Header
            views.secretHeaderTitle.text = header.title
            views.secretHeaderDescription.text = header.description
            views.secretHeaderDescription.setVisible(header.description.isNotEmpty())
            views.secretHeaderVersion.text = views.root.context.getString(
                R.string.secrets_current_version,
                header.currentVersion,
            )
            views.secretHeaderSession.text = views.root.context.getString(
                if (header.sessionActive) R.string.secrets_session_active else R.string.secrets_session_inactive,
            )
            views.secretHeaderDelete.setOnClickListener { actions.delete() }
        }
    }

    private inner class KeyHolder(
        private val views: ItemSecretKeyBinding,
    ) : PathHolder(views, BaseAdapterCallback()) {
        init {
            views.root.applyFullRowRipple()
        }

        override fun bind(item: SecretPathRow) {
            val row = item as SecretPathRow.KeyRow
            views.secretKeyRow.title = row.name
            views.secretKeyRow.subtitle = views.root.context.getString(R.string.secrets_key_version, row.version)
            views.secretKeyRow.value = when {
                row.value != null -> SecretValueText.preview(row.value)
                row.revealing -> views.root.context.getString(R.string.secrets_revealing)
                else -> views.root.context.getString(R.string.secrets_masked_value)
            }
            views.secretKeyCopy.setVisible(row.revealed)
            views.secretKeyRow.setOnClickListener { actions.reveal(row.name) }
            views.secretKeyCopy.setOnClickListener { actions.copy(row.name) }
            views.secretKeyEdit.setOnClickListener { actions.edit(row.name) }
        }
    }

    private inner class VersionHolder(
        private val views: ItemSecretVersionBinding,
    ) : PathHolder(views, BaseAdapterCallback()) {
        override fun bind(item: SecretPathRow) {
            val row = item as SecretPathRow.VersionRow
            views.secretVersionRow.title = views.root.context.getString(R.string.secrets_version, row.version.version)
            views.secretVersionRow.subtitle = listOfNotNull(row.createdLabel, row.unavailableLabel)
                .let(views.root.context::joinMeta)
            views.secretVersionRow.value = if (row.current) views.root.context.getString(R.string.secrets_version_current) else null
            views.secretVersionRollback.text = row.rollbackLabel
            views.secretVersionRollback.setVisible(row.version.available && !row.current)
            views.secretVersionRollback.setOnClickListener { actions.rollback(row.version.version) }
        }
    }

    private class NoteHolder(
        private val views: ItemSecretNoteBinding,
    ) : PathHolder(views, BaseAdapterCallback()) {
        override fun bind(item: SecretPathRow) {
            views.secretNoteText.text = (item as SecretPathRow.Note).text
        }
    }

    private class ValueHolder(
        private val views: ItemSecretValueBinding,
    ) : PathHolder(views, BaseAdapterCallback()) {
        override fun bind(item: SecretPathRow) {
            views.secretValueText.text = (item as SecretPathRow.ValueBody).text
        }
    }

    private companion object {
        const val TYPE_HEADER = 0
        const val TYPE_KEY = 1
        const val TYPE_VERSION = 2
        const val TYPE_NOTE = 3
        const val TYPE_VALUE = 4
    }
}

internal fun keyRows(
    keys: List<String>,
    revealedKey: String?,
    revealedValue: String?,
    version: Long,
    revealingKey: String?,
): List<SecretPathRow> = keys.flatMap { key ->
    val value = revealedValue.takeIf { key == revealedKey }
    listOfNotNull(
        SecretPathRow.KeyRow(
            key = "key:$key",
            name = key,
            value = value,
            revealed = key == revealedKey,
            version = version,
            revealing = key == revealingKey,
        ),
        value?.let(SecretValueText::full)?.let { SecretPathRow.ValueBody("value:$key", it) },
    )
}

internal fun versionRows(
    versions: List<SecretVersion>,
    currentVersion: Long,
    createdLabel: (String) -> String,
    rollbackLabel: String,
    unavailableLabel: (SecretVersion) -> String?,
): List<SecretPathRow> = versions.sortedByDescending { it.version }.map { version ->
    SecretPathRow.VersionRow(
        key = "version:${version.version}",
        version = version,
        current = version.version == currentVersion,
        rollbackLabel = rollbackLabel,
        createdLabel = createdLabel(version.createdTime),
        unavailableLabel = unavailableLabel(version),
    )
}
