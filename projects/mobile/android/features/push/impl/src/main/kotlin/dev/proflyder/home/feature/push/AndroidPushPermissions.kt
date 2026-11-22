package dev.proflyder.home.feature.push

import android.app.NotificationManager
import android.content.Context
import android.os.Build
import dev.proflyder.home.core.ui.NotificationPermission
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class AndroidPushPermissions(
    private val context: Context,
    private val notifications: NotificationPermission,
) : PushPermissionProbe {
    override fun status(): PushPermissionStatus = PushPermissionStatus(
        notifications = notifications.granted(),
        dndAccess = dndAccessGranted(),
        fullScreenIntent = fullScreenIntentGranted(),
    )

    private fun dndAccessGranted(): Boolean =
        context.getSystemService(NotificationManager::class.java)?.isNotificationPolicyAccessGranted == true

    private fun fullScreenIntentGranted(): Boolean = when {
        Build.VERSION.SDK_INT < Build.VERSION_CODES.UPSIDE_DOWN_CAKE -> true
        else -> context.getSystemService(NotificationManager::class.java)?.canUseFullScreenIntent() == true
    }
}
