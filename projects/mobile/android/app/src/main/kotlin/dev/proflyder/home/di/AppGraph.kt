package dev.proflyder.home.di

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.network.NetworkInspection
import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.core.runtime.IoDispatcher
import dev.proflyder.home.core.theme.AppearanceController
import dev.proflyder.home.core.ui.FailureNoticePolicy
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.core.navigation.FeatureRegistry
import dev.proflyder.home.core.settings.HomeDataStore
import dev.proflyder.home.core.settings.KeystoreTokenCipher
import dev.proflyder.home.core.settings.TokenCipher
import dev.proflyder.home.core.ui.ViewModelFactoryRegistry
import dev.proflyder.home.feature.call.CallLauncher
import dev.proflyder.home.feature.push.FindPhoneController
import dev.proflyder.home.feature.push.FirebasePushRegistrar
import dev.proflyder.home.feature.push.NotificationHistoryRecorder
import dev.proflyder.home.feature.push.PushNotifier
import dev.proflyder.home.feature.vpn.VpnProfiles
import dev.proflyder.home.feature.vpn.VpnSettingsStore
import dev.proflyder.home.feature.vpn.tunnel.HomeNetworkMonitor
import dev.proflyder.home.feature.vpn.tunnel.TunnelVpnController
import dev.proflyder.home.feature.vpn.tunnel.VpnOpenConnector
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.BindingContainer
import dev.zacsweers.metro.DependencyGraph
import dev.zacsweers.metro.ElementsIntoSet
import dev.zacsweers.metro.Provides
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.plus
import okhttp3.Interceptor

@DependencyGraph(
    scope = AppScope::class,
    bindingContainers = [AppBindings::class],
)
interface AppGraph {
    val featureRegistry: FeatureRegistry
    val viewModelFactories: ViewModelFactoryRegistry

    val appearance: AppearanceController
    val startables: Set<AppStartable>
    val failureNoticePolicy: FailureNoticePolicy

    val connectionSettings: ConnectionSettingsStore

    val callLauncher: CallLauncher

    val pushRegistrar: FirebasePushRegistrar
    val pushNotifier: PushNotifier
    val findPhoneController: FindPhoneController

    val pushHistory: NotificationHistoryRecorder

    val vpnController: TunnelVpnController
    val vpnProfiles: VpnProfiles
    val vpnSettings: VpnSettingsStore
    val vpnHomeNetwork: HomeNetworkMonitor
    val vpnOpener: VpnOpenConnector

    @DependencyGraph.Factory
    fun interface Factory {
        fun create(@Provides context: Context): AppGraph
    }
}

@BindingContainer
object AppBindings {
    @Provides
    @SingleIn(AppScope::class)
    fun provideDataStore(
        context: Context,
        scope: CoroutineScope,
        @IoDispatcher io: CoroutineDispatcher,
    ): DataStore<Preferences> = HomeDataStore.create(context, scope + io)

    @Provides
    @SingleIn(AppScope::class)
    fun provideTokenCipher(): TokenCipher = KeystoreTokenCipher()

    @Provides
    @ElementsIntoSet
    @NetworkInspection
    fun provideNetworkInspection(context: Context): Set<Interceptor> = NetworkDiagnostics.interceptors(context)

    @Provides
    @SingleIn(AppScope::class)
    fun provideFeatureRegistry(entries: Set<FeatureEntry>): FeatureRegistry =
        FeatureRegistry(entries)
}
