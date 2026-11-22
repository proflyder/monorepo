package dev.proflyder.home.feature.settings.presentation

import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.lifecycle.ViewModel
import dev.proflyder.home.core.designsystem.DesignCatalog
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.StateViewModel
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.core.ui.ViewBinding
import dev.proflyder.home.core.ui.ViewModelSource
import dev.proflyder.home.core.ui.applyScrollBottomInset
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

internal class DesignCatalogViewModel : StateViewModel<Unit, Nothing, Nothing>(Unit) {
    override fun handle(intent: Nothing) = Unit
}

internal class DesignCatalogFragment :
    ScreenFragment<DesignCatalogBinding, Unit, Nothing, Nothing>(DesignCatalogBinding.Companion::inflate) {
    override val viewModel: DesignCatalogViewModel by screenViewModel()

    override fun onScreenCreated(binding: DesignCatalogBinding, savedInstanceState: Bundle?) {
        binding.root.applyScrollBottomInset()
    }

    override fun render(state: Unit) = Unit
}

@ContributesIntoMap(AppScope::class, binding = binding<ViewModelSource>())
@ClassKey(DesignCatalogViewModel::class)
@Inject
class DesignCatalogViewModelSource internal constructor() : ViewModelSource {
    override fun create(): ViewModel = DesignCatalogViewModel()
}
