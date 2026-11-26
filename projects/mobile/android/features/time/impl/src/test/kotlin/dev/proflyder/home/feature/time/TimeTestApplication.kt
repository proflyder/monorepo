package dev.proflyder.home.feature.time

import android.app.Application
import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.TextView
import androidx.fragment.app.Fragment
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.createSavedStateHandle
import androidx.lifecycle.viewmodel.CreationExtras
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.core.navigation.FeatureRegistry
import dev.proflyder.home.core.runtime.AppGraphHolder
import dev.proflyder.home.core.ui.ViewModelFactoryProvider
import dev.proflyder.home.feature.time.presentation.TimeViewModel

class TimeTestApplication : Application(), AppGraphHolder, ViewModelFactoryProvider {
    override val appGraph: Any = object : TimeGraph {
        override val featureRegistry = FeatureRegistry(TimeSection.entries.map(::SectionEntry) + TimeFeatureEntry())
    }

    override val viewModelFactory: ViewModelProvider.Factory = object : ViewModelProvider.Factory {
        override fun <T : ViewModel> create(modelClass: Class<T>, extras: CreationExtras): T =
            modelClass.cast(TimeViewModel(extras.createSavedStateHandle()))!!
    }
}

class TimeTestActivity : FragmentActivity()

class SectionPage : Fragment() {
    override fun onCreateView(inflater: LayoutInflater, container: ViewGroup?, savedInstanceState: Bundle?): View =
        TextView(inflater.context).apply { text = requireArguments().getString(SECTION) }

    companion object {
        const val SECTION = "section"
    }
}

private class SectionEntry(section: TimeSection) : FeatureEntry {
    override val id: String = section.featureId
    override val order: Int = section.ordinal + FIRST_ORDER
    override val isTab: Boolean = false
    override val titleRes: Int = android.R.string.ok
    override val iconRes: Int = android.R.drawable.ic_menu_agenda
    override val rootDestination: Destination = SectionDestination(section.featureId)

    override fun createFragment(destination: Destination): Fragment {
        val section = id
        return SectionPage().apply { arguments = Bundle().apply { putString(SectionPage.SECTION, section) } }
    }

    private companion object {
        const val FIRST_ORDER = 10
    }
}

private data class SectionDestination(override val featureId: String) : Destination {
    override val route: String = featureId
}
