package dev.proflyder.home.feature.chat

import androidx.fragment.app.Fragment
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.feature.chat.impl.R
import dev.proflyder.home.feature.chat.presentation.ChatFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

object ChatDestination : Destination {
    override val featureId: String = ChatRoute.FEATURE_ID
    override val route: String = ChatRoute.ROOT
}

@ContributesIntoSet(AppScope::class)
@Inject
class ChatFeatureEntry : FeatureEntry {
    override val id: String = ChatRoute.FEATURE_ID
    override val order: Int = 3
    override val titleRes: Int = R.string.chat_title
    override val iconRes: Int = R.drawable.chat_ic_tab
    override val rootDestination: Destination = ChatDestination

    override fun supports(destination: Destination): Boolean = destination is ChatDestination

    override fun createFragment(destination: Destination): Fragment = ChatFragment()
}
