package dev.proflyder.home.feature.chat.data

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import dev.proflyder.home.core.network.ChatClient
import dev.proflyder.home.core.network.NetworkWaiter
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineScope

@SingleIn(AppScope::class)
@Inject
class ChatComponents(
    client: ChatClient,
    dataStore: DataStore<Preferences>,
    scope: CoroutineScope,
    networkWaiter: NetworkWaiter,
    context: Context,
) {
    internal val repository: ChatRepository =
        LiveChatRepository(client, DataStoreChatOutboxStore(dataStore), scope, networkWaiter)

    internal val media: ChatMedia = AndroidChatMedia(context, client)
}
