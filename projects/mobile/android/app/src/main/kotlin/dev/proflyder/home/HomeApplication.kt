package dev.proflyder.home

import android.app.Application
import androidx.lifecycle.ViewModelProvider
import com.google.firebase.FirebaseApp
import com.google.firebase.crashlytics.FirebaseCrashlytics
import dev.proflyder.home.core.presentation.ScreenSections
import dev.proflyder.home.core.presentation.ScreenSectionsProvider
import dev.proflyder.home.core.runtime.AppGraphHolder
import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.core.ui.ViewModelFactoryProvider
import dev.proflyder.home.di.AppGraph
import dev.zacsweers.metro.createGraphFactory

class HomeApplication : Application(), AppGraphHolder, ViewModelFactoryProvider, ScreenSectionsProvider {
    lateinit var graph: AppGraph
        private set

    override val appGraph: Any get() = graph

    override val viewModelFactory: ViewModelProvider.Factory get() = graph.viewModelFactory

    override val screenSections: ScreenSections get() = graph.screenSections

    override fun onCreate() {
        super.onCreate()
        graph = createGraphFactory<AppGraph.Factory>().create(this)
        graph.startables.forEach(AppStartable::start)
        configureCrashReporting()
    }

    private fun configureCrashReporting() {
        if (FirebaseApp.getApps(this).isEmpty()) return
        FirebaseCrashlytics.getInstance()
            .setCrashlyticsCollectionEnabled(resources.getBoolean(R.bool.home_crash_reporting))
    }
}
