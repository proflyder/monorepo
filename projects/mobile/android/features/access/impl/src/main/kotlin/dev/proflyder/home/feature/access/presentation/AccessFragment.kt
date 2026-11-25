package dev.proflyder.home.feature.access.presentation

import android.content.DialogInterface
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.widget.LinearLayout
import androidx.appcompat.app.AlertDialog
import androidx.core.view.isVisible
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import com.google.android.material.materialswitch.MaterialSwitch
import dev.proflyder.home.core.designsystem.HomePopupMenu
import dev.proflyder.home.core.model.AccessToken
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.TextBinder
import dev.proflyder.home.core.presentation.bindText
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.SensitiveClipboard
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.access.AccessTokensState
import dev.proflyder.home.feature.access.impl.R
import dev.proflyder.home.feature.access.impl.databinding.DialogAccessCreatedBinding
import dev.proflyder.home.feature.access.impl.databinding.DialogAccessEditorBinding
import dev.proflyder.home.feature.access.impl.databinding.FragmentAccessBinding

internal class AccessFragment :
    ScreenFragment<FragmentAccessBinding, AccessUiState, AccessIntent, AccessEvent>(FragmentAccessBinding.Companion::inflate) {
    override val viewModel: AccessViewModel by screenViewModel()

    private val adapter = AccessAdapter(AccessAdapterCallback(::openToken, ::openMenu))
    private var editorDialog: AlertDialog? = null
    private var editorViews: DialogAccessEditorBinding? = null
    private var editorName: TextBinder? = null
    private var editorKey: String? = null
    private var createdDialog: AlertDialog? = null
    private val clipboard by lazy {
        SensitiveClipboard.forProcess(requireContext(), getString(R.string.access_title))
    }

    override fun onViewReady(binding: FragmentAccessBinding, savedInstanceState: Bundle?) {
        binding.accessList.adapter = adapter
        binding.accessCreate.setOnClickListener { dispatch(AccessIntent.New) }
        binding.accessRefresh.setupPullToRefresh(binding.accessList) { dispatch(AccessIntent.Refresh) }
        binding.accessRetry.setOnClickListener { dispatch(AccessIntent.Refresh) }
        render(AccessUiState::tokens, ::renderStatus)
        render({ it.tokens.tokens }, adapter::submitList)
        render(AccessUiState::editor, ::renderEditor)
        render(AccessUiState::created, ::renderCreated)
    }

    override fun onBindingDestroyed() {
        binding.accessList.adapter = null
        dismissEditor()
        dismissCreated()
    }

    override fun onEvent(event: AccessEvent) {
        when (event) {
            is AccessEvent.ConfirmRevoke -> confirmRevoke(event.token)
            is AccessEvent.Failure -> showFailure(event.failure)
        }
    }

    private fun confirmRevoke(token: AccessToken) {
        confirm(
            R.string.access_revoke_message,
            R.string.access_revoke_yes,
            R.string.access_revoke_keep,
        ) { dispatch(AccessIntent.Revoke(token.id)) }
    }

    private fun openToken(token: AccessToken) = dispatch(AccessIntent.Edit(token))

    private fun openMenu(anchor: View, token: AccessToken) {
        HomePopupMenu(anchor, R.menu.access_token_actions) { onMenuAction(it, token) }.show(Gravity.END)
    }

    private fun onMenuAction(itemId: Int, token: AccessToken) {
        when (itemId) {
            R.id.access_action_edit -> dispatch(AccessIntent.Edit(token))
            R.id.access_action_revoke -> dispatch(AccessIntent.RevokeRequest(token))
        }
    }

    private fun renderStatus(state: AccessTokensState) = with(binding) {
        val failure = state.failure
        accessRefresh.isRefreshing = state.loading && state.tokens.isNotEmpty()
        accessProgress.setVisible(state.loading && state.tokens.isEmpty() && failure == null)
        accessError.setVisible(!state.loading && failure != null && state.tokens.isEmpty())
        accessErrorText.text = failure?.userMessage(requireContext())
        accessEmpty.setVisible(!state.loading && failure == null && state.tokens.isEmpty())
    }

    private fun renderEditor(editor: AccessEditorState?) {
        if (editor == null) {
            dismissEditor()
            return
        }
        val views = editorViews ?: showEditor(editor).also { editorViews = it }
        if (editorKey != (editor.id ?: NEW_EDITOR)) {
            dismissEditor()
            editorViews = showEditor(editor)
            return
        }
        updateEditor(views, editor)
    }

    private fun showEditor(editor: AccessEditorState): DialogAccessEditorBinding {
        editorKey = editor.id ?: NEW_EDITOR
        val views = DialogAccessEditorBinding.inflate(layoutInflater)
        bindModules(views.accessEditorModules)
        editorName = bindText(views.accessEditorName, AccessIntent::EditorName)
        val title = if (editor.id == null) R.string.access_new_title else R.string.access_edit_title
        editorDialog = MaterialAlertDialogBuilder(requireContext())
            .setTitle(title)
            .setView(views.root)
            .setNegativeButton(android.R.string.cancel) { _, _ -> dispatch(AccessIntent.EditorDismiss) }
            .setPositiveButton(R.string.access_editor_save, null)
            .setOnDismissListener { editorName = null }
            .create()
            .also(::showEditorDialog)
        updateEditor(views, editor)
        return views
    }

    private fun showEditorDialog(dialog: AlertDialog) {
        dialog.show()
        dialog.getButton(DialogInterface.BUTTON_POSITIVE).setOnClickListener { dispatch(AccessIntent.EditorSave) }
    }

    private fun bindModules(container: LinearLayout) {
        OFFERED_MODULES.forEach { module -> container.addView(moduleSwitch(container, module)) }
    }

    private fun moduleSwitch(container: LinearLayout, module: AppModule): MaterialSwitch =
        MaterialSwitch(container.context).apply {
            text = context.getString(moduleTitleRes(module))
            tag = module
            setOnCheckedChangeListener { _, checked -> dispatch(AccessIntent.EditorModule(module, checked)) }
        }

    private fun updateEditor(views: DialogAccessEditorBinding, editor: AccessEditorState) {
        editorName?.render(editor.name)
        views.accessEditorName.isEnabled = !editor.saving
        views.accessEditorNameLayout.error = getString(R.string.access_name_error).takeIf { editor.nameError }
        views.accessEditorOwnerNote.isVisible = editor.owner
        views.accessEditorProgress.isVisible = editor.saving
        views.accessEditorError.isVisible = editor.errorRes != null
        views.accessEditorError.text = editor.errorRes?.let(::getString).orEmpty()
        renderModuleSwitches(views.accessEditorModules, editor)
        editorDialog?.getButton(DialogInterface.BUTTON_POSITIVE)?.isEnabled = editor.canSave
    }

    private fun renderModuleSwitches(container: LinearLayout, editor: AccessEditorState) {
        (0 until container.childCount).map(container::getChildAt).filterIsInstance<MaterialSwitch>().forEach { switch ->
            val module = switch.tag as? AppModule ?: return@forEach
            switch.isChecked = module in editor.modules
            switch.isEnabled = editor.canEditModules && !editor.saving
        }
    }

    private fun dismissEditor() {
        editorDialog?.setOnDismissListener(null)
        editorDialog?.dismiss()
        editorDialog = null
        editorViews = null
        editorName = null
        editorKey = null
    }

    private fun renderCreated(created: CreatedAccessState?) {
        if (created == null) {
            dismissCreated()
            return
        }
        if (createdDialog != null) return
        val views = DialogAccessCreatedBinding.inflate(layoutInflater)
        views.accessCreatedToken.text = created.token
        views.accessCreatedToken.contentDescription = getString(R.string.access_token_description, created.name)
        views.accessCreatedCopy.setOnClickListener { copyToken(created) }
        createdDialog = MaterialAlertDialogBuilder(requireContext())
            .setTitle(R.string.access_created_title)
            .setView(views.root)
            .setPositiveButton(R.string.access_created_done) { _, _ -> dispatch(AccessIntent.CreatedDismiss) }
            .setCancelable(false)
            .show()
    }

    private fun copyToken(created: CreatedAccessState) {
        clipboard.copy(created.token)
        showMessage(R.string.access_created_copied)
    }

    private fun dismissCreated() {
        createdDialog?.dismiss()
        createdDialog = null
    }

    private companion object {
        const val NEW_EDITOR = "new"
    }
}
