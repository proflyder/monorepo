package dev.proflyder.home.feature.secrets.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SecretEntry
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.feature.secrets.SecretsCatalogState
import dev.proflyder.home.feature.secrets.SecretsRepository
import dev.proflyder.home.feature.secrets.data.SecretsUnlockSession
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach

internal data class SecretsCatalogGroup(val name: String, val entries: List<SecretEntry>)

internal data class SecretsCatalogUiState(
    val entries: List<SecretEntry> = emptyList(),
    val groups: List<SecretsCatalogGroup> = emptyList(),
    val query: String = "",
    val loading: Boolean = false,
    val truncated: Boolean = false,
    val failure: RemoteFailure? = null,
    val locked: Boolean = true,
    val unlocking: Boolean = false,
)

internal sealed interface SecretsCatalogIntent {
    data object Refresh : SecretsCatalogIntent
    data object RequestAccess : SecretsCatalogIntent
    data class AuthenticationFinished(val granted: Boolean) : SecretsCatalogIntent
    data class Query(val value: String) : SecretsCatalogIntent
    data class Open(val entry: SecretEntry) : SecretsCatalogIntent
    data object OpenDevice : SecretsCatalogIntent
}

internal sealed interface SecretsCatalogEvent {
    data object RequestAuthentication : SecretsCatalogEvent
    data class OpenPath(val path: String) : SecretsCatalogEvent
    data object OpenDevice : SecretsCatalogEvent
}

@Inject
internal class SecretsCatalogViewModel(
    private val repository: SecretsRepository,
    private val unlock: SecretsUnlockSession,
) : StateViewModel<SecretsCatalogUiState, SecretsCatalogIntent, SecretsCatalogEvent>(SecretsCatalogUiState()) {
    private var requested = false

    init {
        combine(repository.catalog, unlock.unlocked) { catalog, unlocked ->
            Triple(catalog, unlocked, currentState.query)
        }
            .onEach { (catalog, unlocked, query) -> apply(catalog, query, unlocked) }
            .launchIn(viewModelScope)
    }

    override fun handle(intent: SecretsCatalogIntent) {
        when (intent) {
            SecretsCatalogIntent.Refresh -> if (!currentState.locked) repository.refresh()
            SecretsCatalogIntent.RequestAccess -> requestAccess()
            is SecretsCatalogIntent.AuthenticationFinished -> finishAuthentication(intent.granted)
            is SecretsCatalogIntent.Query -> updateQuery(intent.value)
            is SecretsCatalogIntent.Open -> emit(SecretsCatalogEvent.OpenPath(intent.entry.path))
            SecretsCatalogIntent.OpenDevice -> emit(SecretsCatalogEvent.OpenDevice)
        }
    }

    private fun requestAccess() {
        if (!currentState.locked) return
        updateState { copy(unlocking = true) }
        emit(SecretsCatalogEvent.RequestAuthentication)
    }

    private fun finishAuthentication(granted: Boolean) {
        if (!granted) {
            updateState { copy(unlocking = false) }
            return
        }
        updateState { copy(unlocking = false) }
        unlock.unlock()
        refreshOnce()
    }

    private fun updateQuery(value: String) {
        val query = value.trim()
        updateState { copy(query = query, groups = group(entries, query)) }
    }

    private fun apply(
        catalog: SecretsCatalogState,
        query: String,
        unlocked: Boolean,
    ) {
        updateState {
            copy(
                entries = catalog.entries,
                groups = group(catalog.entries, query),
                loading = catalog.loading,
                truncated = catalog.truncated,
                failure = catalog.failure,
                locked = !unlocked,
            )
        }
        if (unlocked) refreshOnce()
    }

    private fun refreshOnce() {
        if (requested) return
        requested = true
        repository.refresh()
    }

    private fun group(entries: List<SecretEntry>, query: String): List<SecretsCatalogGroup> =
        entries.filter { it.matches(query) }
            .sortedBy(SecretEntry::path)
            .groupBy(SecretEntry::group)
            .map { (name, paths) -> SecretsCatalogGroup(name, paths) }
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(SecretsCatalogViewModel::class)
@Inject
class SecretsCatalogViewModelSource internal constructor(
    private val viewModel: () -> SecretsCatalogViewModel,
) : ViewModelSource {
    override fun create(): ViewModel = viewModel()
}
