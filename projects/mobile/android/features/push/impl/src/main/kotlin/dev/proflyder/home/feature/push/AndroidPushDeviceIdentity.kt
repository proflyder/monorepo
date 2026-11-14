package dev.proflyder.home.feature.push

import android.annotation.SuppressLint
import android.content.Context
import android.os.Build
import android.provider.Settings
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.feature.push.impl.R
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.nio.charset.StandardCharsets
import java.security.MessageDigest
import java.util.UUID
import kotlinx.coroutines.flow.first

/**
 * Stable identity of this app installation.
 *
 * `Settings.Secure.ANDROID_ID` is scoped to the app signing key and the user,
 * so it survives FCM token rotation and an app reinstall while the phone is
 * not factory-reset. It is hashed with a fixed app-specific salt before it
 * leaves the phone: the server stores a pseudonymous digest, not a raw
 * platform identifier, and the digest still identifies the same phone after a
 * token rotation. If a device reports no `ANDROID_ID`, a random UUID is
 * generated once and persisted, which is equally stable for this installation.
 *
 * A factory reset changes the identity and the phone registers again as a new
 * device; that is the honest outcome, because the old installation is gone.
 */
@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class AndroidPushDeviceIdentity(
    private val context: Context,
    private val dataStore: DataStore<Preferences>,
) : PushDeviceIdentity {
    override suspend fun deviceId(): String {
        val stored = dataStore.data.first()[KEY_DEVICE_ID]
        if (!stored.isNullOrBlank()) return stored
        val generated = generate()
        dataStore.edit { it[KEY_DEVICE_ID] = generated }
        return generated
    }

    override fun defaultName(): String = Build.MODEL.orEmpty().trim()
        .takeUtf8Bytes(NAME_BYTES)
        .ifEmpty { context.getString(R.string.push_default_device_name) }

    // The raw ANDROID_ID never leaves the phone: it is hashed with an app
    // salt below, and only the digest is registered. The lint warning targets
    // code that reports hardware identifiers as-is.
    @SuppressLint("HardwareIds")
    private fun generate(): String {
        val secure = Settings.Secure.getString(context.contentResolver, Settings.Secure.ANDROID_ID)
        val seed = secure?.takeIf { it.isNotBlank() } ?: UUID.randomUUID().toString()
        return digest(seed)
    }

    private fun digest(seed: String): String = MessageDigest.getInstance("SHA-256")
        .digest("$SALT$seed".toByteArray(StandardCharsets.UTF_8))
        .joinToString(separator = "") { byte -> "%02x".format(byte) }

    private companion object {
        /** Matches the scheduler's metadata bound (UTF-8 bytes). */
        const val NAME_BYTES = 64

        /** App-specific salt: the server never sees the raw ANDROID_ID. */
        const val SALT = "dev.proflyder.home/push/"

        val KEY_DEVICE_ID = stringPreferencesKey("push_device_id")
    }
}
