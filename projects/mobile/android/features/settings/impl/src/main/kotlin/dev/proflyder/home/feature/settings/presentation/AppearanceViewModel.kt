package dev.proflyder.home.feature.settings.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.theme.Appearance
import dev.proflyder.home.core.theme.AppearanceController
import dev.proflyder.home.core.theme.ThemeMode
import dev.proflyder.home.core.ui.ViewModelSource
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.StateFlow

@Inject
internal class AppearanceViewModel(private val controller: AppearanceController) : ViewModel() {
    val state: StateFlow<Appearance> = controller.state

    fun selectTheme(theme: AppTheme): Boolean = controller.selectTheme(theme)

    fun selectMode(mode: ThemeMode) = controller.selectMode(mode)
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(AppearanceViewModel::class)
@Inject
class AppearanceViewModelSource internal constructor(
    private val viewModel: () -> AppearanceViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}
