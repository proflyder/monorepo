package dev.proflyder.home.core.runtime

import android.annotation.SuppressLint
import android.content.Context
import android.os.Build
import android.provider.Settings
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.core.model.DeviceIdentity
import dev.proflyder.home.core.text.takeUtf8Bytes
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.nio.charset.StandardCharsets
import java.security.MessageDigest
import java.util.UUID
import kotlinx.coroutines.flow.first

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class AndroidDeviceIdentity(
    private val context: Context,
    private val dataStore: DataStore<Preferences>,
) : DeviceIdentity {
    override suspend fun deviceId(): String {
        val stored = dataStore.data.first()[KEY_DEVICE_ID]
        if (!stored.isNullOrBlank()) return stored
        val generated = generate()
        dataStore.edit { it[KEY_DEVICE_ID] = generated }
        return generated
    }

    override fun defaultName(): String = Build.MODEL.orEmpty().trim()
        .takeUtf8Bytes(NAME_BYTES)
        .ifEmpty { context.getString(R.string.runtime_default_device_name) }

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
        const val NAME_BYTES = 64

        const val SALT = "dev.proflyder.home/push/"

        val KEY_DEVICE_ID = stringPreferencesKey("push_device_id")
    }
}
