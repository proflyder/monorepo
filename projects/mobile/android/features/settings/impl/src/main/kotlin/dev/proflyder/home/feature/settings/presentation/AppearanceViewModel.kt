package dev.proflyder.home.feature.settings.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.presentation.StateViewModel
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
import kotlinx.coroutines.launch

internal sealed interface AppearanceIntent {
    data class SelectTheme(val theme: AppTheme) : AppearanceIntent

    data class SelectMode(val mode: ThemeMode) : AppearanceIntent
}

@Inject
internal class AppearanceViewModel(
    private val controller: AppearanceController,
) : StateViewModel<Appearance, AppearanceIntent, Nothing>(controller.state.value) {
    init {
        viewModelScope.launch { controller.state.collect { appearance -> updateState { appearance } } }
    }

    override fun handle(intent: AppearanceIntent) {
        viewModelScope.launch { apply(intent) }
    }

    private suspend fun apply(intent: AppearanceIntent) = when (intent) {
        is AppearanceIntent.SelectTheme -> controller.selectTheme(intent.theme)
        is AppearanceIntent.SelectMode -> controller.selectMode(intent.mode)
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(AppearanceViewModel::class)
@Inject
class AppearanceViewModelSource internal constructor(
    private val viewModel: () -> AppearanceViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}
