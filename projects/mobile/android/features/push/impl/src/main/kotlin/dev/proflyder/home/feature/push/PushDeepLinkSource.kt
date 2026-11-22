package dev.proflyder.home.feature.push

import android.content.Intent
import dev.proflyder.home.core.navigation.DeepLinkSource
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding

@ContributesIntoSet(AppScope::class, binding = binding<DeepLinkSource>())
@Inject
class PushDeepLinkSource : DeepLinkSource {
    override fun take(intent: Intent): String? {
        val direct = intent.getStringExtra(PushIntents.EXTRA_DEEP_LINK)?.trim().orEmpty()
        val fromTray = PushDeepLink.parse(intent.getStringExtra(PushPayloads.KEY_DEEP_LINK))?.route.orEmpty()
        intent.removeExtra(PushIntents.EXTRA_DEEP_LINK)
        intent.removeExtra(PushPayloads.KEY_DEEP_LINK)
        return direct.ifEmpty { fromTray }.ifEmpty { null }
    }
}
