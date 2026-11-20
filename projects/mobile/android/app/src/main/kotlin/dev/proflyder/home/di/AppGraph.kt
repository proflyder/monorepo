package dev.proflyder.home.di

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.ServerClock
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.core.navigation.FeatureRegistry
import dev.proflyder.home.core.network.ChatClient
import dev.proflyder.home.core.network.ConnectSchedulerClient
import dev.proflyder.home.core.network.ConnectScenarioClient
import dev.proflyder.home.core.network.ConnectIntercomClient
import dev.proflyder.home.core.network.ControlCatalogClient
import dev.proflyder.home.core.network.HttpControlCatalogClient
import dev.proflyder.home.core.network.IntercomClient
import dev.proflyder.home.core.network.HttpPushDevicesClient
import dev.proflyder.home.core.network.NetworkWaiter
import dev.proflyder.home.core.network.PushDevicesApi
import dev.proflyder.home.core.network.ScenarioClient
import dev.proflyder.home.core.network.SchedulerClient
import dev.proflyder.home.core.network.HttpChatClient
import dev.proflyder.home.core.network.HttpSystemDashboardClient
import dev.proflyder.home.core.network.SystemDashboardClient
import dev.proflyder.home.core.settings.HomeDataStore
import dev.proflyder.home.core.settings.KeystoreTokenCipher
import dev.proflyder.home.core.settings.TokenCipher
import dev.proflyder.home.core.theme.AppearanceController
import dev.proflyder.home.core.ui.ViewModelFactoryRegistry
import dev.proflyder.home.feature.call.CallAudioController
import dev.proflyder.home.feature.call.CallController
import dev.proflyder.home.feature.call.CallCoordinator
import dev.proflyder.home.feature.call.CallLauncher
import dev.proflyder.home.feature.call.CallNotificationController
import dev.proflyder.home.feature.call.telecom.CallTelecomBridge
import dev.proflyder.home.feature.push.FindPhoneController
import dev.proflyder.home.feature.push.FirebasePushRegistrar
import dev.proflyder.home.feature.push.NotificationHistoryRecorder
import dev.proflyder.home.feature.push.PushNotifier
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.BindingContainer
import dev.zacsweers.metro.DependencyGraph
import dev.zacsweers.metro.Provides
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import okhttp3.OkHttpClient

@DependencyGraph(
    scope = AppScope::class,
    bindingContainers = [AppBindings::class],
)
interface AppGraph {
    val featureRegistry: FeatureRegistry
    val viewModelFactories: ViewModelFactoryRegistry

    val appearance: AppearanceController

    val connectionSettings: ConnectionSettingsStore

    val httpClient: OkHttpClient

    val callController: CallController
    val callAudioController: CallAudioController
    val callCoordinator: CallCoordinator
    val callNotifications: CallNotificationController
    val callLauncher: CallLauncher
    val callTelecomBridge: CallTelecomBridge

    val pushRegistrar: FirebasePushRegistrar
    val pushNotifier: PushNotifier
    val findPhoneController: FindPhoneController

    val pushHistory: NotificationHistoryRecorder

    @DependencyGraph.Factory
    fun interface Factory {
        fun create(@Provides context: Context): AppGraph
    }
}

@BindingContainer
object AppBindings {
    @Provides
    @SingleIn(AppScope::class)
    fun provideServerClock(): ServerClock = ServerClock(
        localMillis = System::currentTimeMillis,
        elapsedNanos = { android.os.SystemClock.elapsedRealtimeNanos() },
    )

    @Provides
    @SingleIn(AppScope::class)
    fun provideHttpClient(): OkHttpClient = OkHttpClient.Builder()
        .followRedirects(false)
        .followSslRedirects(false)
        .retryOnConnectionFailure(true)
        .build()

    @Provides
    @SingleIn(AppScope::class)
    fun provideNetworkWaiter(context: Context): NetworkWaiter = AndroidNetworkWaiter(context)

    @Provides
    @SingleIn(AppScope::class)
    fun provideAppScope(): CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    @Provides
    @SingleIn(AppScope::class)
    fun provideDataStore(context: Context): DataStore<Preferences> = HomeDataStore.create(context)

    @Provides
    @SingleIn(AppScope::class)
    fun provideTokenCipher(): TokenCipher = KeystoreTokenCipher()

    @Provides
    @SingleIn(AppScope::class)
    fun provideNetworkInterceptors(context: Context): NetworkInterceptors =
        NetworkDiagnostics.interceptors(context)

    @Provides
    @SingleIn(AppScope::class)
    fun provideSchedulerClient(
        settings: ConnectionSettingsStore,
        httpClient: OkHttpClient,
        serverClock: ServerClock,
        interceptors: NetworkInterceptors,
        networkWaiter: NetworkWaiter,
    ): SchedulerClient = ConnectSchedulerClient(
        settings = settings,
        baseClient = httpClient,
        serverClock = serverClock,
        interceptors = interceptors.list,
        networkWaiter = networkWaiter,
    )

    @Provides
    @SingleIn(AppScope::class)
    fun provideScenarioClient(
        settings: ConnectionSettingsStore,
        httpClient: OkHttpClient,
        interceptors: NetworkInterceptors,
        networkWaiter: NetworkWaiter,
    ): ScenarioClient = ConnectScenarioClient(
        settings = settings,
        baseClient = httpClient,
        interceptors = interceptors.list,
        networkWaiter = networkWaiter,
    )

    @Provides
    @SingleIn(AppScope::class)
    fun provideControlCatalogClient(
        settings: ConnectionSettingsStore,
        httpClient: OkHttpClient,
        interceptors: NetworkInterceptors,
        networkWaiter: NetworkWaiter,
    ): ControlCatalogClient = HttpControlCatalogClient(
        settings = settings,
        baseClient = httpClient,
        interceptors = interceptors.list,
        networkWaiter = networkWaiter,
    )

    @Provides
    @SingleIn(AppScope::class)
    fun provideSystemDashboardClient(
        settings: ConnectionSettingsStore,
        httpClient: OkHttpClient,
        interceptors: NetworkInterceptors,
        networkWaiter: NetworkWaiter,
    ): SystemDashboardClient = HttpSystemDashboardClient(
        settings = settings,
        baseClient = httpClient,
        interceptors = interceptors.list,
        networkWaiter = networkWaiter,
    )

    @Provides
    @SingleIn(AppScope::class)
    fun provideChatClient(
        settings: ConnectionSettingsStore,
        httpClient: OkHttpClient,
        interceptors: NetworkInterceptors,
        networkWaiter: NetworkWaiter,
    ): ChatClient = HttpChatClient(
        settings = settings,
        baseClient = httpClient,
        interceptors = interceptors.list,
        networkWaiter = networkWaiter,
    )

    @Provides
    @SingleIn(AppScope::class)
    fun providePushDevicesClient(
        settings: ConnectionSettingsStore,
        httpClient: OkHttpClient,
        interceptors: NetworkInterceptors,
    ): PushDevicesApi = HttpPushDevicesClient(
        settings = settings,
        baseClient = httpClient,
        interceptors = interceptors.list,
    )

    @Provides
    @SingleIn(AppScope::class)
    fun provideIntercomClient(
        settings: ConnectionSettingsStore,
        httpClient: OkHttpClient,
        interceptors: NetworkInterceptors,
        networkWaiter: NetworkWaiter,
    ): IntercomClient = ConnectIntercomClient(
        settings = settings,
        baseClient = httpClient,
        interceptors = interceptors.list,
        networkWaiter = networkWaiter,
    )

    @Provides
    @SingleIn(AppScope::class)
    fun provideFeatureRegistry(entries: Set<FeatureEntry>): FeatureRegistry =
        FeatureRegistry(entries)
}
