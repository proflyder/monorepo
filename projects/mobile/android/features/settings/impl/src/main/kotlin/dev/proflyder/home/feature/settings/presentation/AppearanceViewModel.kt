package dev.proflyder.home.feature.settings.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.theme.Appearance
import dev.proflyder.home.core.theme.AppearanceController
import dev.proflyder.home.core.theme.ThemeMode
import dev.proflyder.home.core.ui.SimpleViewModelFactory
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import kotlinx.coroutines.flow.StateFlow

/** The Appearance screen reads and changes the app-wide choice directly. */
class AppearanceViewModel(private val controller: AppearanceController) : ViewModel() {
    val state: StateFlow<Appearance> = controller.state

    /** True when the activity must be recreated to show the new palette. */
    fun selectTheme(theme: AppTheme): Boolean = controller.selectTheme(theme)

    fun selectMode(mode: ThemeMode) = controller.selectMode(mode)
}

@ContributesIntoSet(AppScope::class)
@Inject
class AppearanceViewModelFactoryContributor(
    private val controller: AppearanceController,
) : ViewModelFactoryContributor {
    override val viewModelClass: Class<out ViewModel> = AppearanceViewModel::class.java

    override fun createFactory() = SimpleViewModelFactory {
        AppearanceViewModel(controller)
    }
}
