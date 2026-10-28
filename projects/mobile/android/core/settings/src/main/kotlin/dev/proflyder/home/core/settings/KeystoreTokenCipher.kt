package dev.proflyder.home.core.settings

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Encrypts the bearer token with a non-exportable Android Keystore AES-256/GCM
 * key. Ciphertext is `base64(iv || ciphertext || tag)`; no plaintext token is
 * ever written to SharedPreferences, logs, SavedState, or a URL.
 */
interface TokenCipher {
    fun encrypt(plaintext: String): String
    fun decrypt(encoded: String): String?
}

class KeystoreTokenCipher(
    private val alias: String = DEFAULT_ALIAS,
) : TokenCipher {
    private val keyStore: KeyStore by lazy {
        KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
    }

    override fun encrypt(plaintext: String): String {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, getOrCreateKey())
        cipher.updateAAD(ASSOCIATED_DATA)
        val ciphertext = cipher.doFinal(plaintext.toByteArray(Charsets.UTF_8))
        val envelope = cipher.iv + ciphertext
        return Base64.encodeToString(envelope, Base64.NO_WRAP)
    }

    /** Returns null when the key was invalidated or the blob is corrupt. */
    override fun decrypt(encoded: String): String? {
        return try {
            val envelope = Base64.decode(encoded, Base64.NO_WRAP)
            if (envelope.size <= IV_BYTES) return null
            val iv = envelope.copyOfRange(0, IV_BYTES)
            val ciphertext = envelope.copyOfRange(IV_BYTES, envelope.size)
            val cipher = Cipher.getInstance(TRANSFORMATION)
            cipher.init(Cipher.DECRYPT_MODE, getOrCreateKey(), GCMParameterSpec(TAG_BITS, iv))
            cipher.updateAAD(ASSOCIATED_DATA)
            String(cipher.doFinal(ciphertext), Charsets.UTF_8)
        } catch (_: Exception) {
            null
        }
    }

    fun deleteKey() {
        try {
            if (keyStore.containsAlias(alias)) keyStore.deleteEntry(alias)
        } catch (_: Exception) {
            // A missing/unavailable keystore needs no cleanup.
        }
    }

    private fun getOrCreateKey(): SecretKey {
        (keyStore.getEntry(alias, null) as? KeyStore.SecretKeyEntry)?.let { return it.secretKey }

        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .setRandomizedEncryptionRequired(true)
                .build(),
        )
        return generator.generateKey()
    }

    companion object {
        private const val ANDROID_KEYSTORE = "AndroidKeyStore"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val DEFAULT_ALIAS = "dev.proflyder.home.connection.token.v1"
        private const val IV_BYTES = 12
        private const val TAG_BITS = 128
        private val ASSOCIATED_DATA = "dev.proflyder.home.connection.v1".toByteArray(Charsets.UTF_8)
    }
}
