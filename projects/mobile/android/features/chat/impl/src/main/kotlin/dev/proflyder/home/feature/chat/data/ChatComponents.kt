package dev.proflyder.home.feature.chat.data

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import dev.proflyder.home.core.network.ChatClient
import dev.proflyder.home.core.network.RouteGate
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CoroutineScope

@SingleIn(AppScope::class)
@Inject
class ChatComponents(
    private val client: ChatClient,
    private val dataStore: DataStore<Preferences>,
    private val scope: CoroutineScope,
    private val routeGate: RouteGate,
    context: Context,
) {
    private val conversations = ConcurrentHashMap<String, ChatRepository>()

    internal val sessions: ChatSessionsRepository = LiveChatSessionsRepository(client, scope)

    internal val media: ChatMedia = AndroidChatMedia(context, client)

    internal val sessionActions: SessionActions = SessionActions(sessions, ::forget)

    internal fun repository(sessionId: String): ChatRepository = conversations.getOrPut(sessionId) {
        LiveChatRepository(sessionId, client, DataStoreChatOutboxStore(dataStore, sessionId), scope, routeGate)
    }

    private fun forget(sessionId: String) {
        conversations.remove(sessionId)
    }
}
