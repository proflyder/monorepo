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
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

internal sealed interface AppearanceIntent {
    data class SelectTheme(val theme: AppTheme) : AppearanceIntent
    data class SelectMode(val mode: ThemeMode) : AppearanceIntent
}

internal sealed interface AppearanceEvent {
    data object ThemeChanged : AppearanceEvent
}

@Inject
internal class AppearanceViewModel(
    private val controller: AppearanceController,
) : StateViewModel<Appearance, AppearanceIntent, AppearanceEvent>(controller.state.value) {
    init {
        controller.state.onEach { appearance -> updateState { appearance } }.launchIn(viewModelScope)
    }

    override fun handle(intent: AppearanceIntent) {
        when (intent) {
            is AppearanceIntent.SelectTheme -> if (controller.selectTheme(intent.theme)) emit(AppearanceEvent.ThemeChanged)
            is AppearanceIntent.SelectMode -> controller.selectMode(intent.mode)
        }
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
