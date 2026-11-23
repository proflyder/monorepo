package dev.proflyder.home.core.launchericon

import androidx.core.app.NotificationCompat

interface ThemedNotifications {
    fun decorate(builder: NotificationCompat.Builder): NotificationCompat.Builder

    fun conversation(builder: NotificationCompat.Builder, title: String, body: String): NotificationCompat.Builder
}
