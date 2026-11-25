package dev.proflyder.home.feature.settings.presentation

import dev.proflyder.home.core.presentation.bindSwitch
import dev.proflyder.home.core.presentation.bindPills
import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.designsystem.DesignCatalog
import dev.proflyder.home.core.designsystem.ListRow
import dev.proflyder.home.core.designsystem.PillSegments
import dev.proflyder.home.core.designsystem.R as DesignR
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.ViewBinding
import dev.proflyder.home.core.ui.ViewModelSource
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ClassKey
import dev.zacsweers.metro.ContributesIntoMap
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding

internal class DesignCatalogBinding(override val root: View) : ViewBinding {
    companion object {
        fun inflate(inflater: LayoutInflater, parent: ViewGroup?, attach: Boolean): DesignCatalogBinding =
            DesignCatalogBinding(DesignCatalog.inflate(inflater, parent).also { if (attach) parent?.addView(it) })
    }
}

internal data class DesignCatalogState(val pill: Int = 0, val switchOn: Boolean = true)

internal sealed interface DesignCatalogIntent {
    data class SelectPill(val index: Int) : DesignCatalogIntent
    data class SetSwitch(val on: Boolean) : DesignCatalogIntent
}

internal class DesignCatalogViewModel :
    StateViewModel<DesignCatalogState, DesignCatalogIntent, Nothing>(DesignCatalogState()) {
    override fun handle(intent: DesignCatalogIntent) = when (intent) {
        is DesignCatalogIntent.SelectPill -> updateState { copy(pill = intent.index) }
        is DesignCatalogIntent.SetSwitch -> updateState { copy(switchOn = intent.on) }
    }
}

internal class DesignCatalogFragment :
    ScreenFragment<DesignCatalogBinding, DesignCatalogState, DesignCatalogIntent, Nothing>(
        DesignCatalogBinding.Companion::inflate,
    ) {
    override val viewModel: DesignCatalogViewModel by screenViewModel()

    override fun onViewReady(binding: DesignCatalogBinding, savedInstanceState: Bundle?) {
        val segments = binding.root.findViewById<PillSegments>(DesignR.id.ds_catalog_pills)
        val pills = bindPills(segments, segments.indices(), DesignCatalogIntent::SelectPill)
        val row = binding.root.findViewById<ListRow>(DesignR.id.ds_catalog_row_switch)
        val switch = bindSwitch(row.switch, DesignCatalogIntent::SetSwitch)
        render(DesignCatalogState::pill, pills::render)
        render(DesignCatalogState::switchOn, switch::render)
    }
}

internal fun PillSegments.indices(): List<Int> = (0 until childCount).toList()

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(DesignCatalogViewModel::class)
@Inject
class DesignCatalogViewModelSource internal constructor() : ViewModelSource {
    override fun create(): ViewModel = DesignCatalogViewModel()
}
