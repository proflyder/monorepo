package dev.proflyder.home.feature.time.presentation

import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.navigation.destination
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.SavedStateViewModelFactory
import dev.proflyder.home.feature.time.TimeDestination
import dev.proflyder.home.feature.time.TimeSection
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding

internal data class TimeUiState(val section: TimeSection)

internal sealed interface TimeIntent {
    data class Select(val section: TimeSection) : TimeIntent
}

internal class TimeViewModel(
    private val saved: SavedStateHandle,
) : StateViewModel<TimeUiState, TimeIntent, Nothing>(TimeUiState(initialSection(saved))) {
    override fun handle(intent: TimeIntent) {
        when (intent) {
            is TimeIntent.Select -> select(intent.section)
        }
    }

    private fun select(section: TimeSection) {
        saved[KEY_SECTION] = section.featureId
        updateState { copy(section = section) }
    }

    private companion object {
        const val KEY_SECTION = "time_section"

        fun initialSection(saved: SavedStateHandle): TimeSection =
            TimeSection.fromFeatureId(saved[KEY_SECTION])
                ?: saved.destination(TimeDestination)?.section
                ?: TimeSection.ALARMS
    }
}

@ContributesIntoMap(AppScope::class, binding = binding<SavedStateViewModelFactory>())
@ClassKey(TimeViewModel::class)
@Inject
class TimeViewModelSource internal constructor() : SavedStateViewModelFactory {
    override fun create(savedState: SavedStateHandle): ViewModel = TimeViewModel(savedState)
}
