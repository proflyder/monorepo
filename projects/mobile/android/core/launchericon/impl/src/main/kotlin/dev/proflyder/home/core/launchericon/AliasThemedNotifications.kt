package dev.proflyder.home.core.launchericon

import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Bitmap
import android.graphics.Canvas
import androidx.core.app.NotificationCompat
import androidx.core.app.Person
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.createBitmap
import androidx.core.graphics.drawable.IconCompat
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.theme.AppearanceController
import dev.proflyder.home.core.theme.NotificationAccent
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class AliasThemedNotifications(
    private val context: Context,
    private val appearance: AppearanceController,
    private val accent: NotificationAccent,
) : ThemedNotifications {
    private val icons = mutableMapOf<AppTheme, Bitmap>()

    override fun decorate(builder: NotificationCompat.Builder): NotificationCompat.Builder =
        builder.setColor(accent.color()).setLargeIcon(icon())

    override fun conversation(
        builder: NotificationCompat.Builder,
        title: String,
        body: String,
    ): NotificationCompat.Builder {
        val sender = sender(title)
        val style = NotificationCompat.MessagingStyle(sender).addMessage(body, System.currentTimeMillis(), sender)
        return builder.setColor(accent.color()).setStyle(style).setShortcutId(publishShortcut())
    }

    private fun sender(title: String): Person = Person.Builder()
        .setName(title)
        .setKey(SHORTCUT_ID)
        .setIcon(IconCompat.createWithBitmap(icon()))
        .build()

    private fun publishShortcut(): String {
        val info = ShortcutInfoCompat.Builder(context, SHORTCUT_ID)
            .setShortLabel(context.applicationInfo.loadLabel(context.packageManager))
            .setLongLived(true)
            .setIcon(IconCompat.createWithBitmap(icon()))
            .setIntent(mainIntent())
            .build()
        ShortcutManagerCompat.pushDynamicShortcut(context, info)
        return SHORTCUT_ID
    }

    private fun mainIntent(): Intent = Intent(Intent.ACTION_MAIN)
        .setComponent(ComponentName(context.packageName, LauncherAliases.TARGET_ACTIVITY))

    private fun icon(): Bitmap = appearance.state.value.theme.let { theme -> icons.getOrPut(theme) { render(theme) } }

    private fun render(theme: AppTheme): Bitmap {
        val drawable = aliasIcon(theme).getOrElse { context.applicationInfo.loadIcon(context.packageManager) }
        val bitmap = createBitmap(ICON_PX, ICON_PX)
        drawable.setBounds(0, 0, ICON_PX, ICON_PX)
        drawable.draw(Canvas(bitmap))
        return bitmap
    }

    private fun aliasIcon(theme: AppTheme) = runCatching {
        val component = ComponentName(context.packageName, LauncherAliases.className(theme))
        context.packageManager.getActivityInfo(component, PackageManager.MATCH_DISABLED_COMPONENTS)
            .loadIcon(context.packageManager)
    }

    private companion object {
        const val SHORTCUT_ID = "home_notifications"
        const val ICON_PX = 192
    }
}
