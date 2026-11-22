package dev.proflyder.home.feature.call

import android.app.Application
import androidx.lifecycle.ViewModelProvider
import dev.proflyder.home.core.ui.ViewModelFactoryProvider

internal class CallTestApplication : Application(), ViewModelFactoryProvider {
    override val viewModelFactory: ViewModelProvider.Factory get() = factory

    companion object {
        lateinit var factory: ViewModelProvider.Factory
    }
}
