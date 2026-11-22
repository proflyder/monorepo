package dev.proflyder.home.feature.vpn.data

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import com.wireguard.crypto.Key
import com.wireguard.crypto.KeyPair
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.flow.first

@SingleIn(AppScope::class)
@Inject
internal class VpnKeyVault(
    private val dataStore: DataStore<Preferences>,
    private val box: VpnSecretBox,
) {
    suspend fun keyPair(): KeyPair = stored() ?: rotate()

    suspend fun rotate(): KeyPair {
        val pair = KeyPair()
        val sealed = box.seal(pair.privateKey.toBase64())
        dataStore.edit { it[KEY_PRIVATE] = sealed }
        return pair
    }

    suspend fun clear() {
        suspendRunCatching { dataStore.edit { it.remove(KEY_PRIVATE) } }
    }

    private suspend fun stored(): KeyPair? {
        val sealed = suspendRunCatching { dataStore.data.first()[KEY_PRIVATE] }.getOrNull()
        val encoded = box.open(sealed) ?: return null
        return runCatching { KeyPair(Key.fromBase64(encoded)) }.getOrNull()
    }

    private companion object {
        val KEY_PRIVATE = stringPreferencesKey("vpn_wireguard_private_key")
    }
}
