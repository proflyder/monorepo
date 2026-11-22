package dev.proflyder.home.core.navigation

import android.content.Intent

fun interface DeepLinkSource {
    fun take(intent: Intent): String?
}

class DeepLinkResolver(private val sources: Set<DeepLinkSource>) {
    fun open(intent: Intent?, navigator: Navigator): Boolean {
        val route = intent?.let { source -> sources.firstNotNullOfOrNull { it.take(source) } } ?: return false
        return navigator.openRoute(route)
    }
}
