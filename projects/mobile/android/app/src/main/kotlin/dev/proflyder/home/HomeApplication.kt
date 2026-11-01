package dev.proflyder.home

import android.app.Application
import com.google.firebase.FirebaseApp
import com.google.firebase.crashlytics.FirebaseCrashlytics
import dev.proflyder.home.di.AppGraph
import dev.zacsweers.metro.createGraphFactory

class HomeApplication : Application() {
    lateinit var graph: AppGraph
        private set

    override fun onCreate() {
        super.onCreate()
        graph = createGraphFactory<AppGraph.Factory>().create(this)
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
