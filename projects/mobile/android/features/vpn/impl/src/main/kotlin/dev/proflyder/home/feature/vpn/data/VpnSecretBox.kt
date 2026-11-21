package dev.proflyder.home.feature.vpn.data

import dev.proflyder.home.core.settings.KeystoreTokenCipher
import dev.proflyder.home.core.settings.TokenCipher
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

@SingleIn(AppScope::class)
@Inject
class VpnSecretBox {
    private val cipher: TokenCipher by lazy { KeystoreTokenCipher(ALIAS) }

    fun seal(plaintext: String): String = cipher.encrypt(plaintext)

    fun open(sealed: String?): String? = sealed?.let(cipher::decrypt)

    private companion object {
        const val ALIAS = "dev.proflyder.home.vpn.secrets.v1"
    }
}
