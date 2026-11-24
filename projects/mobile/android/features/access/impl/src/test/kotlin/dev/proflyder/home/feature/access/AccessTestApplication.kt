package dev.proflyder.home.feature.access

import android.app.Application
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.CreationExtras
import dev.proflyder.home.core.ui.ViewModelFactoryProvider
import dev.proflyder.home.feature.access.presentation.AccessViewModel

class AccessTestApplication : Application(), ViewModelFactoryProvider {
    internal lateinit var viewModel: AccessViewModel

    override val viewModelFactory: ViewModelProvider.Factory = object : ViewModelProvider.Factory {
        override fun <T : ViewModel> create(modelClass: Class<T>): T =
            requireNotNull(modelClass.cast(viewModel))

        override fun <T : ViewModel> create(modelClass: Class<T>, extras: CreationExtras): T = create(modelClass)
    }
}
