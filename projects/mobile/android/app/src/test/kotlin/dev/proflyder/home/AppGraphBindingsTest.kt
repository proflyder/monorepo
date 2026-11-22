package dev.proflyder.home

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class AppGraphBindingsTest {
    @Test
    fun everyScreenViewModelResolvesFromTheGraph() {
        val activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        val provider = ViewModelProvider(activity, (activity.application as HomeApplication).viewModelFactory)
        VIEW_MODELS.forEach { name ->
            val viewModelClass = Class.forName(name).asSubclass(ViewModel::class.java)
            assertEquals(viewModelClass, provider[viewModelClass]::class.java)
        }
    }

    @Test
    fun featureStartablesAreContributed() {
        val app = RuntimeEnvironment.getApplication() as HomeApplication
        val names = app.graph.startables.map { it::class.java.simpleName }.toSet()
        assertTrue(names.toString(), names.containsAll(STARTABLES))
    }

    private companion object {
        val STARTABLES = setOf(
            "CallForegroundStarter",
            "CallAudioSession",
            "FirebasePushRegistrar",
            "TunnelVpnController",
            "HomeNetworkMonitor",
            "VpnOpenConnector",
            "ConnectionHealth",
            "ProcessAppForeground",
            "FirebaseRemoteConnectionSource",
            "AppearanceInstaller",
        )

        val VIEW_MODELS = listOf(
            "dev.proflyder.home.feature.alarms.presentation.AlarmEditorViewModel",
            "dev.proflyder.home.feature.alarms.presentation.AlarmsViewModel",
            "dev.proflyder.home.feature.call.presentation.CallViewModel",
            "dev.proflyder.home.feature.chat.presentation.ChatViewModel",
            "dev.proflyder.home.feature.connection.presentation.ConnectionViewModel",
            "dev.proflyder.home.feature.control.presentation.ControlViewModel",
            "dev.proflyder.home.feature.push.presentation.PushSettingsViewModel",
            "dev.proflyder.home.feature.reminders.presentation.ReminderEditorViewModel",
            "dev.proflyder.home.feature.reminders.presentation.RemindersViewModel",
            "dev.proflyder.home.feature.scenarios.presentation.ScenariosViewModel",
            "dev.proflyder.home.feature.scenarios.presentation.editor.ScenarioEditorViewModel",
            "dev.proflyder.home.feature.settings.presentation.AppearanceViewModel",
            "dev.proflyder.home.feature.settings.presentation.NotificationHistoryViewModel",
            "dev.proflyder.home.feature.system.presentation.SystemViewModel",
            "dev.proflyder.home.feature.timers.presentation.TimersViewModel",
            "dev.proflyder.home.feature.vpn.presentation.VpnViewModel",
        )
    }
}
