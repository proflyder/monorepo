package dev.proflyder.home.feature.vpn.presentation

import android.content.Context
import android.view.LayoutInflater
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.feature.vpn.impl.R
import dev.proflyder.home.feature.vpn.impl.databinding.DialogVpnCertificateBinding

internal class VpnCertificateDialog(
    private val context: Context,
    private val onPassword: (String) -> Unit,
) {
    private val views = DialogVpnCertificateBinding.inflate(LayoutInflater.from(context))

    fun show() {
        MaterialAlertDialogBuilder(context)
            .setTitle(R.string.vpn_certificate_import)
            .setView(views.root)
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(R.string.vpn_certificate_import) { _, _ -> onPassword(password()) }
            .show()
        views.vpnCertificatePassword.requestFocus()
    }

    private fun password(): String = views.vpnCertificatePassword.text?.toString().orEmpty()
}
