package dev.proflyder.home.feature.vpn.presentation

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.result.ActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.designsystem.Tone
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.SwitchBinder
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.vpn.VpnAuth
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnProfileState
import dev.proflyder.home.feature.vpn.VpnRoute
import dev.proflyder.home.feature.vpn.impl.R
import dev.proflyder.home.feature.vpn.impl.databinding.FragmentVpnBinding
import dev.proflyder.home.feature.vpn.isActive

internal class VpnFragment :
    ScreenFragment<FragmentVpnBinding, VpnUiState, VpnIntent, VpnEvent>(FragmentVpnBinding.Companion::inflate) {
    override val viewModel: VpnViewModel by screenViewModel()

    private val permission = registerForActivityResult(ActivityResultContracts.StartActivityForResult(), ::onPermission)

    private val certificatePicker = registerForActivityResult(ActivityResultContracts.OpenDocument(), ::onCertificate)

    private var screen: VpnScreen? = null

    override fun onScreenCreated(binding: FragmentVpnBinding, savedInstanceState: Bundle?) {
        binding.root.applyScrollBottomInset()
        screen = VpnScreen(binding, ::dispatch)
        binding.vpnCertificateImport.setOnClickListener { certificatePicker.launch(CERTIFICATE_TYPES) }
        binding.vpnForget.setOnClickListener { confirmForget() }
    }

    override fun onBindingDestroyed() {
        screen = null
    }

    override fun onResume() {
        super.onResume()
        dispatch(VpnIntent.Refresh)
    }

    override fun render(state: VpnUiState) {
        screen?.render(state, System.currentTimeMillis())
    }

    override fun onEvent(event: VpnEvent) {
        when (event) {
            is VpnEvent.Notice -> showNotice(event.notice)
            is VpnEvent.RequestPermission -> permission.launch(event.request)
            is VpnEvent.Open -> open(event.target)
        }
    }

    private fun onPermission(result: ActivityResult) {
        dispatch(VpnIntent.PermissionResult(result.resultCode == Activity.RESULT_OK))
    }

    private fun onCertificate(uri: Uri?) {
        val picked = uri ?: return
        VpnCertificateDialog(requireContext()) { password -> dispatch(VpnIntent.ImportCertificate(picked, password)) }.show()
    }

    private fun confirmForget() {
        MaterialAlertDialogBuilder(requireContext())
            .setTitle(R.string.vpn_forget_confirm_title)
            .setMessage(R.string.vpn_forget_confirm_body)
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(R.string.vpn_forget) { _, _ -> dispatch(VpnIntent.Forget) }
            .show()
    }

    private fun showNotice(notice: VpnNotice) {
        when (notice) {
            VpnNotice.REISSUED -> showMessage(R.string.vpn_reissued)
            VpnNotice.FORGOTTEN -> showMessage(R.string.vpn_forgotten)
            VpnNotice.CERTIFICATE_IMPORTED -> showMessage(R.string.vpn_certificate_imported)
            VpnNotice.CERTIFICATE_WRONG_PASSWORD -> showError(R.string.vpn_certificate_wrong_password)
            VpnNotice.CERTIFICATE_UNREADABLE -> showError(R.string.vpn_certificate_unreadable)
            VpnNotice.CERTIFICATE_TOO_LARGE -> showError(R.string.vpn_certificate_too_large)
            VpnNotice.PERMISSION_DENIED -> showError(R.string.vpn_permission_denied)
        }
    }

    private fun open(intent: Intent) {
        runCatching { startActivity(intent) }
            .onFailure { failure -> if (failure is ActivityNotFoundException) showError(R.string.vpn_settings_missing) }
    }

    private companion object {
        val CERTIFICATE_TYPES = arrayOf("application/x-pkcs12", "application/octet-stream")
    }
}

internal class VpnScreen(
    private val binding: FragmentVpnBinding,
    dispatch: (VpnIntent) -> Unit,
) {
    private val autoConnect = SwitchBinder(binding.vpnAutoRow.switch) { dispatch(VpnIntent.SetAutoConnect(it)) }

    private val connectOnOpen = SwitchBinder(binding.vpnOpenRow.switch) { dispatch(VpnIntent.SetConnectOnOpen(it)) }

    init {
        labels()
        binding.vpnToggle.setOnClickListener { dispatch(VpnIntent.ToggleConnection) }
        binding.vpnExternal.setOnClickListener { dispatch(VpnIntent.OpenExternal) }
        binding.vpnRoute.onSelected = { index -> dispatch(VpnIntent.SetRoute(VpnRoute.entries[index])) }
        binding.vpnAuth.onSelected = { index -> dispatch(VpnIntent.SetAuth(VpnAuth.entries[index])) }
        binding.vpnCertificateInstall.setOnClickListener { dispatch(VpnIntent.InstallCertificate) }
        binding.vpnReissue.setOnClickListener { dispatch(VpnIntent.Reissue) }
    }

    private fun labels() {
        val context = binding.root.context
        binding.vpnRoute.setLabels(listOf(context.getString(R.string.vpn_route_all), context.getString(R.string.vpn_route_home)))
        binding.vpnAuth.setLabels(listOf(context.getString(R.string.vpn_auth_certificate), context.getString(R.string.vpn_auth_password)))
        binding.vpnCertificateImport.contentDescription = context.getString(R.string.vpn_certificate_import)
    }

    fun render(state: VpnUiState, nowMillis: Long) {
        renderStatus(state.connection, nowMillis, state.busy)
        renderRoute(state)
        renderAuth(state)
        val context = binding.root.context
        binding.vpnProfileRow.subtitle = VpnTexts.profile(context, state.profile)
        binding.vpnProfileRow.subtitleTone = profileTone(state.profile)
        binding.vpnReissue.isEnabled = !state.busy
        binding.vpnForget.isEnabled = !state.busy
    }

    private fun renderStatus(connection: VpnConnection, nowMillis: Long, busy: Boolean) {
        val context = binding.root.context
        binding.vpnStatusRow.title = context.getString(VpnTexts.title(connection))
        binding.vpnStatusRow.subtitle = VpnTexts.detail(context, connection, nowMillis)
        binding.vpnStatusRow.subtitleTone = statusTone(connection)
        val traffic = (connection as? VpnConnection.Connected)?.let { VpnTexts.traffic(context, it) }
        binding.vpnStatusTraffic.text = traffic
        binding.vpnStatusTraffic.setVisible(traffic != null)
        binding.vpnProgress.setVisible(connection is VpnConnection.Connecting || busy)
        binding.vpnToggle.setText(toggleText(connection))
        binding.vpnExternal.setVisible(connection == VpnConnection.FallbackRequired)
    }

    private fun renderRoute(state: VpnUiState) {
        val context = binding.root.context
        binding.vpnRoute.select(state.settings.route.ordinal)
        binding.vpnRouteHelp.text = context.getString(routeHelp(state.settings.route))
        autoConnect.render(state.settings.autoConnect)
        connectOnOpen.render(state.settings.connectOnOpen)
        binding.vpnHomeStatus.text = context.getString(homeText(state.atHome))
    }

    private fun renderAuth(state: VpnUiState) {
        val context = binding.root.context
        val certificate = state.settings.auth == VpnAuth.CERTIFICATE
        binding.vpnAuth.select(state.settings.auth.ordinal)
        binding.vpnExternalStatus.text = context.getString(externalText(state.externalInstalled))
        binding.vpnCertificateRow.setVisible(certificate)
        binding.vpnCertificateRow.subtitle = state.settings.certificateName.ifBlank { context.getString(R.string.vpn_certificate_none) }
        binding.vpnCertificateInstall.setVisible(certificate && state.settings.certificateName.isNotBlank())
        binding.vpnAuthHelp.text = context.getString(authHelp(state.settings.auth))
    }

    private fun toggleText(connection: VpnConnection): Int = when {
        connection.isActive || connection == VpnConnection.FallbackRequired -> R.string.vpn_disconnect
        else -> R.string.vpn_connect
    }

    private fun statusTone(connection: VpnConnection): Tone = when (connection) {
        is VpnConnection.Failed, VpnConnection.FallbackRequired -> Tone.ERROR
        else -> Tone.NEUTRAL
    }

    private fun profileTone(state: VpnProfileState): Tone = when (state) {
        is VpnProfileState.Failed -> Tone.ERROR
        else -> Tone.NEUTRAL
    }

    private fun routeHelp(route: VpnRoute): Int = when (route) {
        VpnRoute.ALL -> R.string.vpn_route_all_help
        VpnRoute.HOME -> R.string.vpn_route_home_help
    }

    private fun authHelp(auth: VpnAuth): Int = when (auth) {
        VpnAuth.CERTIFICATE -> R.string.vpn_auth_certificate_help
        VpnAuth.PASSWORD_TOTP -> R.string.vpn_auth_password_help
    }

    private fun homeText(atHome: Boolean?): Int = when (atHome) {
        null -> R.string.vpn_home_unknown
        true -> R.string.vpn_home_yes
        false -> R.string.vpn_home_no
    }

    private fun externalText(installed: Boolean): Int = when (installed) {
        true -> R.string.vpn_external_ready
        false -> R.string.vpn_external_missing
    }
}
