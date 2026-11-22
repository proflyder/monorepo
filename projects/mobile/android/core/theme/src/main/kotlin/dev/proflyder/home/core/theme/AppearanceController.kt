package dev.proflyder.home.core.theme

import android.app.Application
import androidx.appcompat.app.AppCompatDelegate
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.emptyPreferences
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.core.runtime.IoDispatcher
import dev.proflyder.home.core.runtime.MainDispatcher
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.catch
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import dev.proflyder.home.core.operation.suspendRunCatching

@SingleIn(AppScope::class)
@Inject
class AppearanceController(
    private val dataStore: DataStore<Preferences>,
    private val scope: CoroutineScope,
    @IoDispatcher private val io: CoroutineDispatcher,
    @MainDispatcher private val main: CoroutineDispatcher,
) {
    private val stored: StateFlow<Appearance?> = dataStore.data
        .catch { emit(emptyPreferences()) }
        .map(::decode)
        .flowOn(io)
        .stateIn(scope, SharingStarted.Eagerly, null)

    val state: StateFlow<Appearance> = stored
        .map { it ?: Appearance() }
        .stateIn(scope, SharingStarted.Eagerly, Appearance())

    val loaded: StateFlow<Boolean> = stored
        .map { it != null }
        .stateIn(scope, SharingStarted.Eagerly, false)

    fun install(application: Application) {
        val applier = ThemeApplier(current = { state.value.theme }, ready = loaded)
        application.registerActivityLifecycleCallbacks(applier)
        scope.launch(main) { stored.filterNotNull().distinctUntilChanged().collect { applier.onAppearance(it) } }
    }

    suspend fun selectTheme(theme: AppTheme) {
        persist(KEY_THEME, theme.key)
        state.first { it.theme == theme }
    }

    suspend fun selectMode(mode: ThemeMode) {
        persist(KEY_MODE, mode.key)
        state.first { it.mode == mode }
    }

    private fun decode(preferences: Preferences): Appearance = Appearance(
        theme = AppTheme.fromKey(preferences[KEY_THEME]),
        mode = ThemeMode.fromKey(preferences[KEY_MODE]),
    )

    private suspend fun persist(key: Preferences.Key<String>, value: String) = withContext(io) {
        suspendRunCatching { dataStore.edit { it[key] = value } }
    }

    private companion object {
        val KEY_THEME = stringPreferencesKey("appearance_theme")
        val KEY_MODE = stringPreferencesKey("appearance_mode")
    }
}

internal fun nightModeOf(mode: ThemeMode): Int = when (mode) {
    ThemeMode.SYSTEM -> AppCompatDelegate.MODE_NIGHT_FOLLOW_SYSTEM
    ThemeMode.LIGHT -> AppCompatDelegate.MODE_NIGHT_NO
    ThemeMode.DARK -> AppCompatDelegate.MODE_NIGHT_YES
}
