package dev.proflyder.home.feature.chat

import androidx.fragment.app.Fragment
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.core.navigation.RouteCodec
import dev.proflyder.home.core.navigation.RoutePath
import dev.proflyder.home.core.navigation.ScreenChrome
import dev.proflyder.home.feature.chat.impl.R
import dev.proflyder.home.feature.chat.presentation.ChatFragment
import dev.proflyder.home.feature.chat.presentation.sessions.ChatSessionsFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

internal data class ChatSessionsDestination(val archived: Boolean) : Destination {
    override val featureId: String = ChatRoute.FEATURE_ID
    override val route: String = if (archived) ChatRoute.ARCHIVE else ChatRoute.ROOT

    companion object : RouteCodec<ChatSessionsDestination> {
        val Active = ChatSessionsDestination(archived = false)
        val Archive = ChatSessionsDestination(archived = true)

        override fun decode(path: RoutePath): ChatSessionsDestination? =
            listOf(Active, Archive).firstOrNull { it.route == path.value }
    }
}

internal data class ChatConversationDestination(val sessionId: String) : Destination {
    override val featureId: String = ChatRoute.FEATURE_ID
    override val route: String = ChatRoute.session(sessionId)

    companion object : RouteCodec<ChatConversationDestination> {
        override fun decode(path: RoutePath): ChatConversationDestination? =
            ChatRoute.parse(path.value)?.sessionId?.let(::ChatConversationDestination)
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class ChatFeatureEntry : FeatureEntry {
    override val id: String = ChatRoute.FEATURE_ID
    override val order: Int = 3
    override val module: AppModule = AppModule.CHAT
    override val titleRes: Int = R.string.chat_title
    override val iconRes: Int = R.drawable.chat_ic_tab
    override val rootDestination: Destination = ChatSessionsDestination.Active

    override val routes: List<RouteCodec<*>> = listOf(
        ChatSessionsDestination,
        ChatConversationDestination,
    )

    override fun titleRes(destination: Destination): Int = when (destination) {
        ChatSessionsDestination.Archive -> R.string.chat_archive_title
        else -> R.string.chat_title
    }

    override fun screenChrome(destination: Destination): ScreenChrome = when (destination) {
        is ChatConversationDestination -> ScreenChrome.IMMERSIVE
        else -> ScreenChrome.FULL
    }

    override fun deepLink(path: RoutePath): Destination? {
        ChatRoute.parse(path.value)?.messageId?.let(ChatFocus::request)
        return parse(path)
    }

    override fun createFragment(destination: Destination): Fragment = when (destination) {
        is ChatConversationDestination -> ChatFragment()
        else -> ChatSessionsFragment()
    }
}
