package dev.proflyder.home.feature.push

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.datastore.preferences.core.Preferences
import dev.proflyder.home.core.data.FeatureLifetime
import dev.proflyder.home.core.data.FeatureScope
import dev.proflyder.home.core.data.Lease
import dev.proflyder.home.feature.push.impl.R
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeout
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class NotificationHistoryStoreTest {
    @Test
    fun theNewestMessageIsFirstAndSurvivesARestart() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val dataStore = store(context, "history-order")
        val store = DataStoreNotificationHistoryStore(dataStore, lifetime())

        store.record(entry("first", NotificationHistoryType.FIND, 1_000))
        store.record(entry("second", NotificationHistoryType.NOTIFY, 2_000))

        val reloaded = DataStoreNotificationHistoryStore(dataStore, lifetime())

        assertEquals(listOf("second", "first"), reloaded.settled(2).map { it.id })
        assertEquals(NotificationHistoryType.FIND, reloaded.entries.value.last().type)
    }

    @Test
    fun anUnloadedStoreDoesNotOverwriteEarlierMessages() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val dataStore = store(context, "history-unloaded")
        DataStoreNotificationHistoryStore(dataStore, lifetime())
            .record(entry("earlier", NotificationHistoryType.NOTIFY, 1_000))

        val writer = DataStoreNotificationHistoryStore(dataStore, lifetime())
        writer.record(entry("later", NotificationHistoryType.NOTIFY, 2_000))

        val reader = DataStoreNotificationHistoryStore(dataStore, lifetime())

        assertEquals(listOf("later", "earlier"), reader.settled(2).map { it.id })
    }

    @Test
    fun onlyTheNewestTwoHundredMessagesAreKept() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val store = DataStoreNotificationHistoryStore(store(context, "history-limit"), lifetime())
        repeat(NotificationHistoryRules.MAX_ENTRIES + 5) { index ->
            store.record(entry("m-$index", NotificationHistoryType.NOTIFY, index.toLong()))
        }

        assertEquals(NotificationHistoryRules.MAX_ENTRIES, store.settled(NotificationHistoryRules.MAX_ENTRIES, "m-204").size)
        assertEquals("m-204", store.entries.value.first().id)
        assertTrue(store.entries.value.none { it.id == "m-0" })
    }

    @Test
    fun newMessagesAreUnreadAndReadStateSurvivesARestart() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val dataStore = store(context, "history-read")
        val store = DataStoreNotificationHistoryStore(dataStore, lifetime())
        store.record(entry("a", NotificationHistoryType.NOTIFY, 1_000))
        store.record(entry("b", NotificationHistoryType.NOTIFY, 2_000))
        assertEquals(2, NotificationHistoryRules.unread(store.settled(2)))

        store.markRead("a")
        val afterOne = DataStoreNotificationHistoryStore(dataStore, lifetime())
        assertEquals(listOf(false, true), afterOne.settledUnread(1).map { it.read })

        afterOne.markAllRead()
        val afterAll = DataStoreNotificationHistoryStore(dataStore, lifetime())
        assertEquals(0, NotificationHistoryRules.unread(afterAll.settledUnread(0)))
    }

    @Test
    fun entriesSavedBeforeReadStateExistedCountAsRead() {
        val legacy = checkNotNull(javaClass.getResource("/fixtures/legacy_history.json")).readText()
        assertTrue(NotificationHistoryCodec.decode(legacy).single().read)
    }

    @Test
    fun clearingRemovesEveryMessage() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val dataStore = store(context, "history-clear")
        val store = DataStoreNotificationHistoryStore(dataStore, lifetime())
        store.record(entry("kept", NotificationHistoryType.NOTIFY, 1_000))

        store.clear()

        val reloaded = DataStoreNotificationHistoryStore(dataStore, lifetime())
        assertTrue(reloaded.settled(0).isEmpty())
    }

    @Test
    fun aFindRequestIsRecordedWithTheLocalizedTitle() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val store = DataStoreNotificationHistoryStore(store(context, "history-find"), lifetime())
        val recorder = NotificationHistoryRecorder(context, FixedHistory(store))

        recorder.record(PushMessage.Find(reason = " "))

        val recorded = store.settled(1).single()
        assertEquals(NotificationHistoryType.FIND, recorded.type)
        assertEquals(context.getString(R.string.push_find_title), recorded.title)
        assertEquals(context.getString(R.string.push_find_default_reason), recorded.body)
        assertNull(recorded.deepLink)
    }

    @Test
    fun aNotificationKeepsItsTextAndDeepLink() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val store = DataStoreNotificationHistoryStore(store(context, "history-notify"), lifetime())
        val recorder = NotificationHistoryRecorder(context, FixedHistory(store))

        recorder.record(
            PushMessage.Notify(
                title = "Front door",
                body = "Someone is at the door",
                deepLink = PushDeepLink.parse("home://alarms"),
            ),
        )

        val recorded = store.settled(1).single()
        assertEquals(NotificationHistoryType.NOTIFY, recorded.type)
        assertEquals("Front door", recorded.title)
        assertEquals("alarms", recorded.deepLink)
    }

    private fun lifetime() = FeatureLifetime(CoroutineScope(Dispatchers.Unconfined + SupervisorJob()))

    private suspend fun NotificationHistoryStore.settled(size: Int, first: String? = null): List<NotificationHistoryEntry> =
        withTimeout(TIMEOUT_MILLIS) { entries.first { it.size == size && (first == null || it.first().id == first) } }

    private suspend fun NotificationHistoryStore.settledUnread(unread: Int): List<NotificationHistoryEntry> =
        withTimeout(TIMEOUT_MILLIS) { entries.first { it.size == 2 && NotificationHistoryRules.unread(it) == unread } }

    private class FixedHistory(private val store: NotificationHistoryStore) : FeatureScope<NotificationHistoryStore> {
        override val owners: Int = 0

        override val instance: NotificationHistoryStore = store

        override fun acquire(): Lease<NotificationHistoryStore> = Lease(store) { }
    }

    private fun entry(id: String, type: NotificationHistoryType, millis: Long): NotificationHistoryEntry =
        NotificationHistoryEntry(
            id = id,
            type = type,
            title = "title",
            body = "body",
            receivedAtMillis = millis,
        )

    private fun store(context: Context, name: String): DataStore<Preferences> =
        PreferenceDataStoreFactory.create(
            scope = CoroutineScope(Dispatchers.IO + SupervisorJob()),
            produceFile = { File(context.filesDir, "$name.preferences_pb") },
        )

    private companion object {
        const val TIMEOUT_MILLIS = 5_000L
    }
}
