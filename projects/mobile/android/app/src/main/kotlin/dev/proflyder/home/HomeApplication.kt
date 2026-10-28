package dev.proflyder.home

import android.app.Application
import dev.proflyder.home.di.AppGraph
import dev.zacsweers.metro.createGraphFactory

class HomeApplication : Application() {
    lateinit var graph: AppGraph
        private set

    override fun onCreate() {
        super.onCreate()
        graph = createGraphFactory<AppGraph.Factory>().create(this)
    }
}
