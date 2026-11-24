package dev.proflyder.home.feature.push

import android.content.Intent
import dev.proflyder.home.core.navigation.DeepLinkSource
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding

@ContributesIntoSet(AppScope::class, binding = binding<DeepLinkSource>())
@Inject
class PushDeepLinkSource(private val token: PushIntentToken) : DeepLinkSource {
    override fun take(intent: Intent): String? {
        val authorized = token.matches(intent.getStringExtra(PushIntents.EXTRA_DEEP_LINK_TOKEN))
        val direct = if (authorized) {
            intent.getStringExtra(PushIntents.EXTRA_DEEP_LINK)
                ?.let { PushDeepLink.parse("${PushDeepLink.SCHEME}://$it") }
                ?.route
                .orEmpty()
        } else {
            ""
        }
        val fromTray = if (authorized) {
            intent.getStringExtra(PushPayloads.KEY_DEEP_LINK)
                ?.let(PushDeepLink::parse)
                ?.route
                .orEmpty()
        } else {
            ""
        }
        intent.removeExtra(PushIntents.EXTRA_DEEP_LINK)
        intent.removeExtra(PushIntents.EXTRA_DEEP_LINK_TOKEN)
        intent.removeExtra(PushPayloads.KEY_DEEP_LINK)
        return direct.ifEmpty { fromTray }.ifEmpty { null }
    }
}
