package dev.proflyder.home.feature.call.presentation

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.content.ContextCompat
import dev.zacsweers.metro.Inject

internal interface CallPermissions {
    fun missing(): List<String>

    fun microphoneGranted(): Boolean
}

@Inject
internal class AndroidCallPermissions(private val context: Context) : CallPermissions {
    override fun missing(): List<String> = wanted().filterNot(::granted)

    override fun microphoneGranted(): Boolean = granted(Manifest.permission.RECORD_AUDIO)

    private fun wanted(): List<String> = buildList {
        add(Manifest.permission.RECORD_AUDIO)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) add(Manifest.permission.BLUETOOTH_CONNECT)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) add(Manifest.permission.POST_NOTIFICATIONS)
    }

    private fun granted(permission: String): Boolean =
        ContextCompat.checkSelfPermission(context, permission) == PackageManager.PERMISSION_GRANTED
}
