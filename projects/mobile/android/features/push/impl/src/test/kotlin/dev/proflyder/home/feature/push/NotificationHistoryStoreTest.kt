package dev.proflyder.home.feature.push

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.datastore.preferences.core.Preferences
import dev.proflyder.home.feature.push.impl.R
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

/**
 * The history is written by the FCM service, which can run before any screen
 * loaded the log. These tests use the real DataStore file, so a message is
 * never lost by an unbounded read-modify-write.
 */
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class NotificationHistoryStoreTest {
    @Test
    fun theNewestMessageIsFirstAndSurvivesARestart() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val dataStore = store(context, "history-order")
        val store = DataStoreNotificationHistoryStore(dataStore)

        store.record(entry("first", NotificationHistoryType.FIND, 1_000))
        store.record(entry("second", NotificationHistoryType.NOTIFY, 2_000))

        val reloaded = DataStoreNotificationHistoryStore(dataStore)
        reloaded.load()

        assertEquals(listOf("second", "first"), reloaded.entries.value.map { it.id })
        assertEquals(NotificationHistoryType.FIND, reloaded.entries.value.last().type)
    }

    @Test
    fun anUnloadedStoreDoesNotOverwriteEarlierMessages() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val dataStore = store(context, "history-unloaded")
        DataStoreNotificationHistoryStore(dataStore)
            .record(entry("earlier", NotificationHistoryType.NOTIFY, 1_000))

        // The service process records without ever calling load().
        val writer = DataStoreNotificationHistoryStore(dataStore)
        writer.record(entry("later", NotificationHistoryType.NOTIFY, 2_000))

        val reader = DataStoreNotificationHistoryStore(dataStore)
        reader.load()

        assertEquals(listOf("later", "earlier"), reader.entries.value.map { it.id })
    }

    @Test
    fun onlyTheNewestTwoHundredMessagesAreKept() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val store = DataStoreNotificationHistoryStore(store(context, "history-limit"))
        repeat(NotificationHistoryRules.MAX_ENTRIES + 5) { index ->
            store.record(entry("m-$index", NotificationHistoryType.NOTIFY, index.toLong()))
        }

        assertEquals(NotificationHistoryRules.MAX_ENTRIES, store.entries.value.size)
        assertEquals("m-204", store.entries.value.first().id)
        assertTrue(store.entries.value.none { it.id == "m-0" })
    }

    @Test
    fun clearingRemovesEveryMessage() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val dataStore = store(context, "history-clear")
        val store = DataStoreNotificationHistoryStore(dataStore)
        store.record(entry("kept", NotificationHistoryType.NOTIFY, 1_000))

        store.clear()

        val reloaded = DataStoreNotificationHistoryStore(dataStore)
        reloaded.load()
        assertTrue(reloaded.entries.value.isEmpty())
    }

    @Test
    fun aFindRequestIsRecordedWithTheLocalizedTitle() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val store = DataStoreNotificationHistoryStore(store(context, "history-find"))
        val recorder = NotificationHistoryRecorder(context, store)

        recorder.record(PushMessage.Find(reason = " "))

        val recorded = store.entries.value.single()
        assertEquals(NotificationHistoryType.FIND, recorded.type)
        assertEquals(context.getString(R.string.push_find_title), recorded.title)
        assertEquals(context.getString(R.string.push_find_default_reason), recorded.body)
        assertNull(recorded.deepLink)
    }

    @Test
    fun aNotificationKeepsItsTextAndDeepLink() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val store = DataStoreNotificationHistoryStore(store(context, "history-notify"))
        val recorder = NotificationHistoryRecorder(context, store)

        recorder.record(
            PushMessage.Notify(
                title = "Front door",
                body = "Someone is at the door",
                deepLink = PushDeepLink.parse("home://alarms"),
            ),
        )

        val recorded = store.entries.value.single()
        assertEquals(NotificationHistoryType.NOTIFY, recorded.type)
        assertEquals("Front door", recorded.title)
        assertEquals("alarms", recorded.deepLink)
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
}
