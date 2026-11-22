package dev.proflyder.home

import android.app.Application
import com.google.firebase.FirebaseApp
import com.google.firebase.crashlytics.FirebaseCrashlytics
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.core.ui.ViewModelFactoryProvider
import dev.proflyder.home.di.AppGraph
import dev.proflyder.home.feature.push.PushEntryPoint
import dev.proflyder.home.feature.vpn.VpnEntryPoint
import dev.zacsweers.metro.createGraphFactory

class HomeApplication : Application(), ViewModelFactoryProvider {
    lateinit var graph: AppGraph
        private set

    override fun onCreate() {
        super.onCreate()
        graph = createGraphFactory<AppGraph.Factory>().create(this)
        graph.startables.forEach(AppStartable::start)
        graph.connectionSettings
        PushEntryPoint.install(
            PushEntryPoint.Dependencies(
                registrar = graph.pushRegistrar,
                notifications = graph.pushNotifier,
                findPhone = graph.findPhoneController,
                history = graph.pushHistory,
            ),
        )
        graph.pushRegistrar.start()
        VpnEntryPoint.install(
            VpnEntryPoint.Dependencies(
                controller = graph.vpnController,
                profiles = graph.vpnProfiles,
                settings = graph.vpnSettings,
                homeNetwork = graph.vpnHomeNetwork,
                opener = graph.vpnOpener,
            ),
        )
        configureCrashReporting()
    }

    override fun viewModelFactory(viewModelClass: Class<out ViewModel>): ViewModelProvider.Factory =
        graph.viewModelFactories.viewModelFactory(viewModelClass)

    private fun configureCrashReporting() {
        if (FirebaseApp.getApps(this).isEmpty()) return
        FirebaseCrashlytics.getInstance()
            .setCrashlyticsCollectionEnabled(resources.getBoolean(R.bool.home_crash_reporting))
    }
}
