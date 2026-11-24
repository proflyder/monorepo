package dev.proflyder.home.feature.secrets

import android.app.Application
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.CreationExtras
import dev.proflyder.home.core.ui.ViewModelFactoryProvider
import dev.proflyder.home.feature.secrets.presentation.SecretsCatalogViewModel

class SecretsTestApplication : Application(), ViewModelFactoryProvider {
    internal var viewModel: ViewModel? = null

    override val viewModelFactory: ViewModelProvider.Factory = object : ViewModelProvider.Factory {
        override fun <T : ViewModel> create(modelClass: Class<T>): T =
            requireNotNull(modelClass.cast(requireNotNull(viewModel)))

        override fun <T : ViewModel> create(modelClass: Class<T>, extras: CreationExtras): T = create(modelClass)
    }
}

class SecretsTestActivity : FragmentActivity()

internal val catalogViewModel: Class<SecretsCatalogViewModel> = SecretsCatalogViewModel::class.java
