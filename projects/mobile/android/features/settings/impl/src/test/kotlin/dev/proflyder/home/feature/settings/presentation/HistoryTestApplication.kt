package dev.proflyder.home.feature.settings.presentation

import android.app.Application
import android.os.Bundle
import android.view.Gravity
import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.widget.FrameLayout
import dev.proflyder.home.core.designsystem.glass.GlassChromeLayout
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.CreationExtras
import dev.proflyder.home.core.ui.ViewModelFactoryProvider

class HistoryTestApplication : Application(), ViewModelFactoryProvider {
    internal var viewModel: ViewModel? = null

    override val viewModelFactory: ViewModelProvider.Factory = object : ViewModelProvider.Factory {
        override fun <T : ViewModel> create(modelClass: Class<T>): T = requireNotNull(modelClass.cast(viewModel))

        override fun <T : ViewModel> create(modelClass: Class<T>, extras: CreationExtras): T = create(modelClass)
    }
}

class HistoryTestActivity : FragmentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val root = FrameLayout(this)
        val container = FrameLayout(this).apply { id = CONTAINER_ID }
        val chrome = GlassChromeLayout(this).apply { minimumHeight = CHROME_HEIGHT }
        root.addView(container, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
        root.addView(chrome, FrameLayout.LayoutParams(MATCH_PARENT, CHROME_HEIGHT, Gravity.TOP))
        chrome.bindBackdrop(container)
        setContentView(root)
    }

    companion object {
        val CONTAINER_ID = View.generateViewId()
        const val CHROME_HEIGHT = 300
    }
}
