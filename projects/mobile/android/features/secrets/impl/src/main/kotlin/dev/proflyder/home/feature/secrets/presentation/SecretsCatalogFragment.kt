package dev.proflyder.home.feature.secrets.presentation

import android.content.Intent
import android.os.Bundle
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import dev.proflyder.home.core.navigation.Navigator
import dev.proflyder.home.core.navigation.NavigatorHost
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.core.navigation.ScreenActionSink
import dev.proflyder.home.core.navigation.ToolbarAction
import dev.proflyder.home.core.presentation.bindText
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.setupPullToRefresh
import dev.proflyder.home.core.ui.userMessage
import dev.proflyder.home.feature.secrets.SecretsDeviceDestination
import dev.proflyder.home.feature.secrets.SecretsPathDestination
import dev.proflyder.home.feature.secrets.impl.R
import dev.proflyder.home.feature.secrets.impl.databinding.FragmentSecretsCatalogBinding

internal class SecretsCatalogFragment :
    SecretsScreenFragment<FragmentSecretsCatalogBinding, SecretsCatalogUiState, SecretsCatalogIntent, SecretsCatalogEvent>(
        FragmentSecretsCatalogBinding.Companion::inflate,
    ) {
    override val viewModel: SecretsCatalogViewModel by screenViewModel()

    private val adapter by lazy {
        SecretsCatalogAdapter(
            onClick = { row -> dispatch(SecretsCatalogIntent.Open(row.entry)) },
            onBindDevice = { dispatch(SecretsCatalogIntent.OpenDevice) },
        )
    }

    private val deviceAuth by lazy { DeviceAuth(requireActivity()) }

    private val deviceMenuAction = object : ToolbarAction {
        override val itemId: Int = R.id.secrets_device_action
        override val order: Int = 0
        override val titleRes: Int = R.string.secrets_device_menu
        override val iconRes: Int = R.drawable.secrets_ic_device

        override fun perform(navigator: Navigator) {
            dispatch(SecretsCatalogIntent.OpenDevice)
        }
    }

    private val screenActions: ScreenActionSink?
        get() = (activity as? NavigatorHost)?.navigator as? ScreenActionSink

    private val credentials: ActivityResultLauncher<Intent> =
        registerForActivityResult(ActivityResultContracts.StartActivityForResult()) { result ->
            dispatch(SecretsCatalogIntent.AuthenticationFinished(result.resultCode == android.app.Activity.RESULT_OK))
        }

    override fun onViewReady(binding: FragmentSecretsCatalogBinding, savedInstanceState: Bundle?) {
        binding.secretsCatalogList.adapter = adapter
        binding.secretsCatalogList.applyScrollBottomInset()
        binding.secretsCatalogRefresh.setupPullToRefresh(binding.secretsCatalogList) {
            dispatch(SecretsCatalogIntent.Refresh)
        }
        binding.secretsCatalogRetry.setOnClickListener { dispatch(SecretsCatalogIntent.Refresh) }
        binding.secretsCatalogUnlock.setOnClickListener { dispatch(SecretsCatalogIntent.RequestAccess) }
        val search = bindText(binding.secretsCatalogSearch) { SecretsCatalogIntent.Query(it) }
        render({ it.query }) { search.render(it) }
        render(::rows) { adapter.submitList(it) }
        render(::menuActions) { screenActions?.publishScreenActions(this, it) }
        render(::status) { renderStatus(it) }
        dispatch(SecretsCatalogIntent.RefreshDevice)
        if (viewModel.state.value.locked) dispatch(SecretsCatalogIntent.RequestAccess)
    }

    private fun rows(state: SecretsCatalogUiState): List<SecretsCatalogRow> =
        if (state.locked) emptyList() else catalogRows(state.groups, state.deviceNotEnrolled)

    private fun menuActions(state: SecretsCatalogUiState): List<ToolbarAction> =
        if (state.deviceEnrolled) listOf(deviceMenuAction) else emptyList()

    private fun status(state: SecretsCatalogUiState): CatalogStatus = CatalogStatus(
        locked = state.locked,
        loading = state.loading,
        hasEntries = state.entries.isNotEmpty(),
        failure = state.failure,
        truncated = state.truncated,
    )

    private fun renderStatus(status: CatalogStatus) = with(binding) {
        secretsCatalogLocked.setVisible(status.locked)
        secretsCatalogSearchLayout.setVisible(!status.locked)
        secretsCatalogRefresh.setVisible(!status.locked)
        secretsCatalogProgress.setVisible(!status.locked && status.loading && !status.hasEntries)
        secretsCatalogError.setVisible(!status.locked && !status.loading && !status.hasEntries && status.failure != null)
        secretsCatalogErrorText.text = status.failure?.userMessage(requireContext()).orEmpty()
        secretsCatalogEmpty.setVisible(!status.locked && !status.loading && !status.hasEntries && status.failure == null)
        secretsCatalogTruncated.setVisible(!status.locked && status.truncated)
        secretsCatalogRefresh.isRefreshing = !status.locked && status.loading && status.hasEntries
    }

    override fun onEvent(event: SecretsCatalogEvent) {
        when (event) {
            SecretsCatalogEvent.RequestAuthentication -> authenticate()
            is SecretsCatalogEvent.OpenPath ->
                navigate(Route.Screen(SecretsPathDestination(event.path)))
            SecretsCatalogEvent.OpenDevice ->
                navigate(Route.Screen(SecretsDeviceDestination))
        }
    }

    private fun authenticate() {
        deviceAuth.authenticate(
            credentials = ::launchCredentials,
            result = { granted ->
                dispatch(SecretsCatalogIntent.AuthenticationFinished(granted))
            },
        )
    }

    private fun launchCredentials(intent: Intent) {
        runCatching { credentials.launch(intent) }
            .onFailure { dispatch(SecretsCatalogIntent.AuthenticationFinished(false)) }
    }

    override fun onBindingDestroyed() {
        screenActions?.clearScreenActions(this)
        super.onBindingDestroyed()
    }

    override fun onDestroy() {
        deviceAuth.cancel()
        super.onDestroy()
    }
}

internal data class CatalogStatus(
    val locked: Boolean,
    val loading: Boolean,
    val hasEntries: Boolean,
    val failure: dev.proflyder.home.core.model.RemoteFailure?,
    val truncated: Boolean,
)
