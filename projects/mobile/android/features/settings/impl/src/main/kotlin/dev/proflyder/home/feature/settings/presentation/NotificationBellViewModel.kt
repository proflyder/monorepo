package dev.proflyder.home.feature.settings.presentation

import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.data.FeatureScope
import dev.proflyder.home.core.presentation.viewModel
import dev.proflyder.home.core.ui.BaseViewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.push.NotificationHistoryRepository
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding

internal class NotificationBellViewModel(repository: NotificationHistoryRepository) : BaseViewModel<Int>(0) {
    init {
        launch { repository.unread.collect { unread -> updateState { unread } } }
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(NotificationBellViewModel::class)
@Inject
class NotificationBellViewModelSource internal constructor(
    private val history: FeatureScope<NotificationHistoryRepository>,
) : ViewModelSource {
    override fun create(): ViewModel = history.viewModel(::NotificationBellViewModel)
}
