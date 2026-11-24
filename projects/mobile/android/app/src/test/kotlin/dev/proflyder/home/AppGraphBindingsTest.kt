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

    @Test
    fun chromeAndDeepLinksAreContributedByFeatures() {
        val activity = Robolectric.buildActivity(MainActivity::class.java).setup().get()
        assertEquals(
            setOf("NotificationBellContribution", "VoiceProfileToolbarChip", "VpnToolbarContribution"),
            activity.toolbarContributions.map { it::class.java.simpleName }.toSet(),
        )
        assertEquals(
            setOf("CallToolbarAction", "SettingsToolbarAction"),
            activity.toolbarActions.map { it::class.java.simpleName }.toSet(),
        )
        assertTrue(activity.features.entry("time") != null)
    }

    @Test
    fun aPushDeepLinkOpensTheTypedRoute() {
        val intent = android.content.Intent(RuntimeEnvironment.getApplication(), MainActivity::class.java)
            .putExtra(dev.proflyder.home.feature.push.PushIntents.EXTRA_DEEP_LINK, "settings/history")
        val activity = Robolectric.buildActivity(MainActivity::class.java, intent).setup().get()
        org.robolectric.Shadows.shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()
        assertEquals("settings/history", activity.supportFragmentManager.findFragmentById(R.id.fragment_container)?.tag)
        assertEquals(null, activity.intent.getStringExtra(dev.proflyder.home.feature.push.PushIntents.EXTRA_DEEP_LINK))
    }

    private companion object {
        val STARTABLES = setOf(
            "CallForegroundStarter",
            "CallAudioSession",
            "HomePushRegistrar",
            "FirebasePushTokenSource",
            "PushInbox",
            "ChainVpnController",
            "AutoConnectCoordinator",
            "VpnStatusServiceStarter",
            "VpnProfileSync",
            "ConnectionHealth",
            "ProcessAppForeground",
            "FirebaseRemoteConnectionSource",
            "AppearanceInstaller",
            "AppAccessSession",
        )

        val VIEW_MODELS = listOf(
            "dev.proflyder.home.feature.access.presentation.AccessViewModel",
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
            "dev.proflyder.home.feature.settings.presentation.DesignCatalogViewModel",
            "dev.proflyder.home.feature.settings.presentation.SettingsViewModel",
            "dev.proflyder.home.feature.settings.presentation.NotificationHistoryViewModel",
            "dev.proflyder.home.feature.system.presentation.SystemViewModel",
            "dev.proflyder.home.feature.time.presentation.TimeViewModel",
            "dev.proflyder.home.feature.timers.presentation.TimersViewModel",
            "dev.proflyder.home.feature.vpn.presentation.VpnViewModel",
        )
    }
}
