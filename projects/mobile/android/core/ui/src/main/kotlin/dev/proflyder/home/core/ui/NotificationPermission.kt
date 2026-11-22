package dev.proflyder.home.core.ui

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

interface NotificationPermission {
    fun granted(): Boolean
}

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class AndroidNotificationPermission(private val context: Context) : NotificationPermission {
    override fun granted(): Boolean = runtimeGranted() && NotificationManagerCompat.from(context).areNotificationsEnabled()

    private fun runtimeGranted(): Boolean = when {
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU -> true
        else -> ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) ==
            PackageManager.PERMISSION_GRANTED
    }
}
