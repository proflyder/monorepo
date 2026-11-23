package dev.proflyder.home.feature.push

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.core.runtime.AppStartable
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import dev.zacsweers.metro.binding
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

@SingleIn(AppScope::class)
@ContributesIntoSet(AppScope::class, binding = binding<AppStartable>())
@Inject
internal class LegacyNotificationHistoryCleanup(
    private val dataStore: DataStore<Preferences>,
    private val scope: CoroutineScope,
) : AppStartable {
    override fun start() {
        scope.launch { clean() }
    }

    internal suspend fun clean() {
        dataStore.edit { preferences -> preferences.remove(KEY) }
    }

    private companion object {
        val KEY = stringPreferencesKey("push_notification_history")
    }
}
