package dev.proflyder.home.feature.push

import android.Manifest
import android.app.NotificationManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.provider.Settings
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.core.net.toUri

/**
 * Reads and fixes the three permissions the loud path depends on.
 *
 * They are granted in different places, so each row of the settings screen has
 * its own real state and its own system settings page:
 *
 * | Permission | Source of truth | Fix |
 * | --- | --- | --- |
 * | notifications | `POST_NOTIFICATIONS` (13+) / enabled channels | runtime dialog or app notification settings |
 * | DND bypass | `NotificationManager.isNotificationPolicyAccessGranted` | `ACTION_NOTIFICATION_POLICY_ACCESS_SETTINGS` |
 * | full-screen intent | `canUseFullScreenIntent` (14+) | `ACTION_MANAGE_APP_USE_FULL_SCREEN_INTENT` |
 */
class PushPermissions(private val context: Context) {

    /** Current state as the screen renders it. */
    fun status(): PushPermissionStatus = PushPermissionStatus(
        notifications = notificationsGranted(),
        dndAccess = dndAccessGranted(),
        fullScreenIntent = fullScreenIntentGranted(),
    )

    /** The runtime dialog intent, when the platform still asks. */
    fun notifications(): Intent = Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
        .putExtra(Settings.EXTRA_APP_PACKAGE, context.packageName)

    /** Do Not Disturb access lives outside the app's own settings page. */
    fun dnd(): Intent = Intent(Settings.ACTION_NOTIFICATION_POLICY_ACCESS_SETTINGS)

    /** Full-screen intent allowance (Android 14+). */
    fun fullScreen(): Intent = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
        Intent(Settings.ACTION_MANAGE_APP_USE_FULL_SCREEN_INTENT)
            .setData("package:${context.packageName}".toUri())
    } else {
        appDetails()
    }

    /** App details, the fallback for every permission on older releases. */
    fun appDetails(): Intent = Intent(
        Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
        "package:${context.packageName}".toUri(),
    ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)

    private fun notificationsGranted(): Boolean {
        val granted = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            ContextCompat.checkSelfPermission(
                context,
                Manifest.permission.POST_NOTIFICATIONS,
            ) == PackageManager.PERMISSION_GRANTED
        } else {
            true
        }
        return granted && NotificationManagerCompat.from(context).areNotificationsEnabled()
    }

    private fun dndAccessGranted(): Boolean {
        val manager = context.getSystemService(NotificationManager::class.java) ?: return false
        return manager.isNotificationPolicyAccessGranted
    }

    private fun fullScreenIntentGranted(): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.UPSIDE_DOWN_CAKE) return true
        val manager = context.getSystemService(NotificationManager::class.java) ?: return false
        return manager.canUseFullScreenIntent()
    }
}
