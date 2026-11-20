package dev.proflyder.home.feature.settings.presentation

import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.fragment.app.Fragment
import dev.proflyder.home.core.designsystem.DesignCatalog
import dev.proflyder.home.core.ui.applyScrollBottomInset

internal class DesignCatalogFragment : Fragment() {
    override fun onCreateView(inflater: LayoutInflater, container: ViewGroup?, savedInstanceState: Bundle?): View =
        DesignCatalog.inflate(inflater, container)

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        view.applyScrollBottomInset()
    }
}
