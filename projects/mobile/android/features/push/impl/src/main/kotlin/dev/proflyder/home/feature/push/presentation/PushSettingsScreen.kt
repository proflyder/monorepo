package dev.proflyder.home.feature.push.presentation

import android.content.Context
import android.content.Intent
import android.os.Build
import android.provider.Settings
import androidx.core.net.toUri

internal enum class PushSettingsScreen {
    NOTIFICATIONS,
    DND,
    FULL_SCREEN,
    ;

    fun intent(context: Context): Intent = when (this) {
        NOTIFICATIONS -> Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
            .putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)
        DND -> Intent(Settings.ACTION_NOTIFICATION_POLICY_ACCESS_SETTINGS)
        FULL_SCREEN -> fullScreen(context)
    }

    private fun fullScreen(context: Context): Intent = when {
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE ->
            Intent(Settings.ACTION_MANAGE_APP_USE_FULL_SCREEN_INTENT).setData(packageUri(context))
        else -> Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, packageUri(context))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    }

    private fun packageUri(context: Context) = "package:${context.packageName}".toUri()
}
