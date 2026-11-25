package dev.proflyder.home.feature.secrets.presentation

import android.content.Context
import android.content.SharedPreferences
import androidx.core.content.edit
import java.security.MessageDigest

internal class StagingAliasState(private val context: Context) {
    fun staged(): String? = preferences().getString(ALIAS_KEY, null)

    fun stagedChallenge(): String? = preferences().getString(CHALLENGE_KEY, null)

    fun stage(alias: String, challenge: String) {
        preferences().edit(commit = true) {
            putString(ALIAS_KEY, alias)
            putString(CHALLENGE_KEY, challenge)
        }
    }

    fun clear() {
        preferences().edit(commit = true) {
            remove(ALIAS_KEY)
            remove(CHALLENGE_KEY)
        }
    }

    private fun preferences() = context.deviceKeyPreferences()

    private companion object {
        const val ALIAS_KEY = "staging_alias"
        const val CHALLENGE_KEY = "staging_challenge"
    }
}

internal fun Context.deviceKeyPreferences(): SharedPreferences =
    getSharedPreferences(DEVICE_KEY_PREFERENCES, Context.MODE_PRIVATE)

private const val DEVICE_KEY_PREFERENCES = "home_secrets_device"

internal fun canReuseStagedKey(
    stagedAlias: String?,
    stagedChallenge: String?,
    challenge: String,
): Boolean = stagedAlias != null && stagedChallenge == challenge

internal fun samePublicKey(first: String?, second: String?): Boolean {
    if (first == null || second == null) return false
    return MessageDigest.isEqual(first.toByteArray(), second.toByteArray())
}
