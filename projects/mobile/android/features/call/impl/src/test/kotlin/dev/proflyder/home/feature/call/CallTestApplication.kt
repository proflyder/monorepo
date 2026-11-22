package dev.proflyder.home.feature.call

import android.app.Application
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import dev.proflyder.home.core.ui.ViewModelFactoryProvider

internal class CallTestApplication : Application(), ViewModelFactoryProvider {
    override fun viewModelFactory(viewModelClass: Class<out ViewModel>): ViewModelProvider.Factory = factory

    companion object {
        lateinit var factory: ViewModelProvider.Factory
    }
}
