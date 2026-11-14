package dev.proflyder.home

import android.app.Application
import com.google.firebase.FirebaseApp
import com.google.firebase.crashlytics.FirebaseCrashlytics
import dev.proflyder.home.di.AppGraph
import dev.proflyder.home.feature.call.CallEntryPoint
import dev.proflyder.home.feature.push.PushEntryPoint
import dev.zacsweers.metro.createGraphFactory

class HomeApplication : Application() {
    lateinit var graph: AppGraph
        private set

    override fun onCreate() {
        super.onCreate()
        graph = createGraphFactory<AppGraph.Factory>().create(this)
        // Instantiate the connection store at launch so the Remote Config
        // source sets defaults and starts its first fetch.
        graph.connectionSettings
        // Framework-instantiated components (ConnectionService, call Activity,
        // foreground service) cannot be constructor-injected by Metro.
        CallEntryPoint.install(
            CallEntryPoint.Dependencies(
                controller = graph.callController,
                audioController = graph.callAudioController,
                coordinator = graph.callCoordinator,
                telecomBridge = graph.callTelecomBridge,
            ),
        )
        graph.callNotifications.start()
        // Framework-instantiated push components (FCM service, ringing service
        // and screen) read their collaborators from this entry point.
        PushEntryPoint.install(
            PushEntryPoint.Dependencies(
                registrar = graph.pushRegistrar,
                notifications = graph.pushNotifier,
                findPhone = graph.findPhoneController,
                history = graph.pushHistory,
            ),
        )
        // Registers the FCM token (again) on every start and creates channels.
        graph.pushRegistrar.start()
        configureCrashReporting()
    }

    /**
     * Crashlytics auto-initializes from the merged Firebase manifests when
     * google-services resources are present. Without them FirebaseApp has no
     * default app; skip configuration so a clean checkout still launches.
     */
    private fun configureCrashReporting() {
        if (FirebaseApp.getApps(this).isEmpty()) return
        FirebaseCrashlytics.getInstance()
            .setCrashlyticsCollectionEnabled(resources.getBoolean(R.bool.home_crash_reporting))
    }
}
