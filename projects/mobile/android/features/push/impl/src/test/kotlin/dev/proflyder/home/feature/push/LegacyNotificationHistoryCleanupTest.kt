package dev.proflyder.home.feature.push

import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import java.io.File
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class LegacyNotificationHistoryCleanupTest {
    @Test
    fun theOldHistoryKeyIsRemovedOnce() = runBlocking {
        val context = RuntimeEnvironment.getApplication()
        val scope = CoroutineScope(Dispatchers.IO)
        val dataStore = PreferenceDataStoreFactory.create(scope = scope) {
            File(context.filesDir, "legacy-history-cleanup.preferences_pb")
        }
        dataStore.edit { preferences -> preferences[KEY] = "old" }

        LegacyNotificationHistoryCleanup(dataStore, scope).clean()

        assertNull(dataStore.data.first()[KEY])
    }

    private companion object {
        val KEY = stringPreferencesKey("push_notification_history")
    }
}
