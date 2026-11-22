package dev.proflyder.home.feature.vpn.data

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.security.KeyChain
import android.util.Base64
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.proflyder.home.feature.vpn.VpnSettingsStore
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.security.KeyStore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withContext

internal enum class CertificateImport {
    IMPORTED,
    WRONG_PASSWORD,
    UNREADABLE,
    TOO_LARGE,
}

@SingleIn(AppScope::class)
@Inject
internal class VpnCertificateStore(
    private val context: Context,
    private val dataStore: DataStore<Preferences>,
    private val box: VpnSecretBox,
    private val settings: VpnSettingsStore,
) {
    suspend fun import(uri: Uri, password: String): CertificateImport = withContext(Dispatchers.IO) {
        val bytes = read(uri) ?: return@withContext CertificateImport.UNREADABLE
        when {
            bytes.size > MAX_BYTES -> CertificateImport.TOO_LARGE
            else -> verifyAndSave(bytes, password)
        }
    }

    suspend fun installIntent(): Intent? {
        val stored = suspendRunCatching { dataStore.data.first() }.getOrNull() ?: return null
        val bytes = box.open(stored[KEY_P12])?.let { Base64.decode(it, Base64.NO_WRAP) } ?: return null
        val name = settings.settings.value?.certificateName.orEmpty()
        return KeyChain.createInstallIntent()
            .putExtra(KeyChain.EXTRA_PKCS12, bytes)
            .putExtra(KeyChain.EXTRA_NAME, name)
    }

    suspend fun clear() {
        suspendRunCatching { dataStore.edit { it.remove(KEY_P12) } }
        settings.setCertificateName("")
    }

    private suspend fun verifyAndSave(bytes: ByteArray, password: String): CertificateImport {
        val alias = aliasOf(bytes, password) ?: return CertificateImport.WRONG_PASSWORD
        val sealed = box.seal(Base64.encodeToString(bytes, Base64.NO_WRAP))
        suspendRunCatching { dataStore.edit { it[KEY_P12] = sealed } }
        settings.setCertificateName(alias)
        return CertificateImport.IMPORTED
    }

    private fun aliasOf(bytes: ByteArray, password: String): String? = runCatching {
        val store = KeyStore.getInstance(PKCS12)
        store.load(bytes.inputStream(), password.toCharArray())
        store.aliases().toList().firstOrNull { store.isKeyEntry(it) }
    }.getOrNull()

    private fun read(uri: Uri): ByteArray? = runCatching {
        context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
    }.getOrNull()

    private companion object {
        const val PKCS12 = "PKCS12"
        const val MAX_BYTES = 64 * 1024
        val KEY_P12 = stringPreferencesKey("vpn_ocserv_p12")
    }
}
