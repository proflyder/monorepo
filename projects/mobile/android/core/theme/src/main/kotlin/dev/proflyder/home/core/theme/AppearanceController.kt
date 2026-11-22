package dev.proflyder.home.core.theme

import android.app.Application
import androidx.appcompat.app.AppCompatDelegate
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import dev.proflyder.home.core.runtime.IoDispatcher
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking

@SingleIn(AppScope::class)
@Inject
class AppearanceController(
    private val dataStore: DataStore<Preferences>,
    private val scope: CoroutineScope,
    @IoDispatcher private val io: CoroutineDispatcher,
) {
    private val mutableState = MutableStateFlow(Appearance())

    val state: StateFlow<Appearance> = mutableState.asStateFlow()

    fun install(application: Application) {
        mutableState.value = readStored()
        applyMode(state.value.mode)
        application.registerActivityLifecycleCallbacks(ThemeApplier { state.value.theme })
    }

    fun selectTheme(theme: AppTheme): Boolean {
        if (state.value.theme == theme) return false
        mutableState.value = state.value.copy(theme = theme)
        persist(KEY_THEME, theme.key)
        return true
    }

    fun selectMode(mode: ThemeMode) {
        if (state.value.mode == mode) return
        mutableState.value = state.value.copy(mode = mode)
        persist(KEY_MODE, mode.key)
        applyMode(mode)
    }

    private fun readStored(): Appearance = runCatching { runBlocking { dataStore.data.first() } }
        .map(::decode)
        .getOrDefault(Appearance())

    private fun decode(preferences: Preferences): Appearance = Appearance(
        theme = AppTheme.fromKey(preferences[KEY_THEME]),
        mode = ThemeMode.fromKey(preferences[KEY_MODE]),
    )

    private fun persist(key: Preferences.Key<String>, value: String) {
        scope.launch(io) { write(key, value) }
    }

    internal suspend fun write(key: Preferences.Key<String>, value: String) {
        runCatching { dataStore.edit { it[key] = value } }
    }

    private fun applyMode(mode: ThemeMode) {
        AppCompatDelegate.setDefaultNightMode(nightMode(mode))
    }

    private fun nightMode(mode: ThemeMode): Int = when (mode) {
        ThemeMode.SYSTEM -> AppCompatDelegate.MODE_NIGHT_FOLLOW_SYSTEM
        ThemeMode.LIGHT -> AppCompatDelegate.MODE_NIGHT_NO
        ThemeMode.DARK -> AppCompatDelegate.MODE_NIGHT_YES
    }

    private companion object {
        val KEY_THEME = stringPreferencesKey("appearance_theme")
        val KEY_MODE = stringPreferencesKey("appearance_mode")
    }
}
