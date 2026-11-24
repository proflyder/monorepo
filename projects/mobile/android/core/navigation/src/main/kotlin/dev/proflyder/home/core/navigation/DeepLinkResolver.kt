package dev.proflyder.home.core.navigation

import android.content.Intent
import dev.proflyder.home.core.access.AppAccessSource

fun interface DeepLinkSource {
    fun take(intent: Intent): String?
}

class DeepLinkResolver(
    private val sources: Set<DeepLinkSource>,
    private val targets: NotificationTargets,
    private val access: AppAccessSource,
) {
    fun open(intent: Intent?, navigator: Navigator): Boolean {
        val link = intent?.let { source -> sources.firstNotNullOfOrNull { it.take(source) } } ?: return false
        val route = targets.of(link)?.routeWhenAllowed(access.state.value) ?: return false
        navigator.open(route)
        return true
    }
}
