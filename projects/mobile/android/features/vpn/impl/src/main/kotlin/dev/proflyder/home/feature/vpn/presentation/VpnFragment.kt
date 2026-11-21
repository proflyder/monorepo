package dev.proflyder.home.feature.vpn.presentation

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import androidx.activity.result.ActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.fragment.app.viewModels
import androidx.lifecycle.lifecycleScope
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.designsystem.Tone
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.applyScrollBottomInset
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.vpn.VpnAuth
import dev.proflyder.home.feature.vpn.VpnConnection
import dev.proflyder.home.feature.vpn.VpnProfileState
import dev.proflyder.home.feature.vpn.VpnRoute
import dev.proflyder.home.feature.vpn.impl.R
import dev.proflyder.home.feature.vpn.impl.databinding.FragmentVpnBinding
import dev.proflyder.home.feature.vpn.isActive
import kotlinx.coroutines.launch

class VpnFragment : BaseFragment<FragmentVpnBinding>(FragmentVpnBinding.Companion::inflate) {
    private val viewModel: VpnViewModel by viewModels { viewModelFactory(VpnViewModel::class.java) }

    private val permission = registerForActivityResult(ActivityResultContracts.StartActivityForResult(), ::onPermission)

    private val certificatePicker = registerForActivityResult(ActivityResultContracts.OpenDocument(), ::onCertificate)

    override fun onBindingCreated(binding: FragmentVpnBinding, savedInstanceState: Bundle?) {
        binding.root.applyScrollBottomInset()
        VpnScreenBinder.labels(binding)
        binding.vpnToggle.setOnClickListener { onToggle() }
        binding.vpnExternal.setOnClickListener { viewModel.openExternal() }
        binding.vpnRoute.onSelected = { index -> viewModel.setRoute(VpnRoute.entries[index]) }
        binding.vpnAuth.onSelected = { index -> viewModel.setAuth(VpnAuth.entries[index]) }
        binding.vpnAutoRow.switch.setOnClickListener { viewModel.setAutoConnect(binding.vpnAutoRow.switch.isChecked) }
        binding.vpnCertificateImport.setOnClickListener { certificatePicker.launch(CERTIFICATE_TYPES) }
        binding.vpnCertificateInstall.setOnClickListener { installCertificate() }
        binding.vpnReissue.setOnClickListener { viewModel.reissue() }
        binding.vpnForget.setOnClickListener { confirmForget() }
        viewModel.state.observe { render(it) }
    }

    override fun onResume() {
        super.onResume()
        viewModel.refresh()
    }

    private fun render(state: VpnUiState) {
        VpnScreenBinder.render(binding, state, System.currentTimeMillis())
        state.notice?.let { notice ->
            showNotice(notice)
            viewModel.consumeNotice()
        }
    }

    private fun onToggle() {
        val request = viewModel.permissionIntent()
        when {
            viewModel.state.value.connection.isActive -> viewModel.toggle()
            request != null -> permission.launch(request)
            else -> viewModel.connect()
        }
    }

    private fun onPermission(result: ActivityResult) {
        when (result.resultCode) {
            Activity.RESULT_OK -> viewModel.connect()
            else -> showError(R.string.vpn_permission_denied)
        }
    }

    private fun onCertificate(uri: Uri?) {
        val picked = uri ?: return
        VpnCertificateDialog(requireContext()) { password -> viewModel.importCertificate(picked, password) }.show()
    }

    private fun installCertificate() {
        viewLifecycleOwner.lifecycleScope.launch {
            viewModel.installIntent()?.let(::open)
        }
    }

    private fun confirmForget() {
        MaterialAlertDialogBuilder(requireContext())
            .setTitle(R.string.vpn_forget_confirm_title)
            .setMessage(R.string.vpn_forget_confirm_body)
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(R.string.vpn_forget) { _, _ -> viewModel.forget() }
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

internal object VpnScreenBinder {
    fun labels(binding: FragmentVpnBinding) {
        val context = binding.root.context
        binding.vpnRoute.setLabels(listOf(context.getString(R.string.vpn_route_all), context.getString(R.string.vpn_route_home)))
        binding.vpnAuth.setLabels(listOf(context.getString(R.string.vpn_auth_certificate), context.getString(R.string.vpn_auth_password)))
        binding.vpnCertificateImport.contentDescription = context.getString(R.string.vpn_certificate_import)
    }

    fun render(binding: FragmentVpnBinding, state: VpnUiState, nowMillis: Long) {
        renderStatus(binding, state.connection, nowMillis, state.busy)
        renderRoute(binding, state)
        renderAuth(binding, state)
        val context = binding.root.context
        binding.vpnProfileRow.subtitle = VpnTexts.profile(context, state.profile)
        binding.vpnProfileRow.subtitleTone = profileTone(state.profile)
        binding.vpnReissue.isEnabled = !state.busy
        binding.vpnForget.isEnabled = !state.busy
    }

    private fun renderStatus(binding: FragmentVpnBinding, connection: VpnConnection, nowMillis: Long, busy: Boolean) {
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

    private fun renderRoute(binding: FragmentVpnBinding, state: VpnUiState) {
        val context = binding.root.context
        binding.vpnRoute.select(state.settings.route.ordinal)
        binding.vpnRouteHelp.text = context.getString(routeHelp(state.settings.route))
        binding.vpnAutoRow.switch.isChecked = state.settings.autoConnect
        binding.vpnHomeStatus.text = context.getString(homeText(state.atHome))
    }

    private fun renderAuth(binding: FragmentVpnBinding, state: VpnUiState) {
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
