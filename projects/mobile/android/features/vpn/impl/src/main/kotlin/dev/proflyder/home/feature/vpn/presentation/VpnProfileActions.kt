package dev.proflyder.home.feature.vpn.presentation

import android.content.Intent
import android.net.Uri
import dev.proflyder.home.feature.vpn.VpnController
import dev.proflyder.home.feature.vpn.VpnProfiles
import dev.proflyder.home.feature.vpn.data.CertificateImport
import dev.proflyder.home.feature.vpn.data.VpnCertificateStore
import dev.zacsweers.metro.Inject

@Inject
internal class VpnProfileActions(
    private val controller: VpnController,
    private val profiles: VpnProfiles,
    private val certificates: VpnCertificateStore,
) {
    suspend fun reissue(): VpnNotice? = when (profiles.reissue()) {
        true -> VpnNotice.REISSUED.also { controller.reconnect() }
        false -> null
    }

    suspend fun forget(): VpnNotice? {
        controller.disconnect()
        return when (profiles.forget()) {
            true -> VpnNotice.FORGOTTEN.also { certificates.clear() }
            false -> null
        }
    }

    suspend fun importCertificate(uri: Uri, password: String): VpnNotice =
        when (certificates.import(uri, password)) {
            CertificateImport.IMPORTED -> VpnNotice.CERTIFICATE_IMPORTED
            CertificateImport.WRONG_PASSWORD -> VpnNotice.CERTIFICATE_WRONG_PASSWORD
            CertificateImport.UNREADABLE -> VpnNotice.CERTIFICATE_UNREADABLE
            CertificateImport.TOO_LARGE -> VpnNotice.CERTIFICATE_TOO_LARGE
        }

    suspend fun certificateInstall(): Intent? = certificates.installIntent()
}
