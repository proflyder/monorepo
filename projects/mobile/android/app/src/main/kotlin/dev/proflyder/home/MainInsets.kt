package dev.proflyder.home

import android.view.View
import androidx.core.view.OnApplyWindowInsetsListener
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.isVisible
import androidx.core.view.updatePadding
import dev.proflyder.home.core.ui.doOnApplyInsets
import dev.proflyder.home.core.ui.horizontalInsets
import dev.proflyder.home.core.ui.keyboardVisible
import dev.proflyder.home.core.ui.navigationInset
import dev.proflyder.home.core.ui.topInset
import dev.proflyder.home.core.ui.underChrome
import dev.proflyder.home.databinding.ActivityMainBinding

internal fun ActivityMainBinding.applySystemBarInsets(tabs: BottomTabs): ChromeSizes {
    topChrome.doOnApplyInsets { view, insets, initial ->
        val sides = insets.horizontalInsets()
        view.updatePadding(left = sides.left, top = initial.top + insets.topInset(), right = sides.right)
    }
    bottomChrome.doOnApplyInsets { view, insets, initial ->
        val sides = insets.horizontalInsets()
        view.updatePadding(left = sides.left, right = sides.right, bottom = initial.bottom + insets.navigationInset())
    }
    ViewCompat.setOnApplyWindowInsetsListener(homeNoticeAnchor) { _, insets -> insets }
    ViewCompat.setOnApplyWindowInsetsListener(root) { _, insets -> insets.also { tabs.keyboard(it.keyboardVisible()) } }
    val sizes = ChromeSizes(this)
    ViewCompat.setOnApplyWindowInsetsListener(fragmentContainer, sizes)
    topChrome.addOnLayoutChangeListener(sizes)
    bottomChrome.addOnLayoutChangeListener(sizes)
    return sizes
}

internal fun ActivityMainBinding.showChrome(shown: Boolean, sizes: ChromeSizes) {
    if (topChrome.isVisible == shown && bottomChrome.isVisible == shown) return
    topChrome.isVisible = shown
    bottomChrome.isVisible = shown
    sizes.redispatch()
}

internal class ChromeSizes(private val binding: ActivityMainBinding) : View.OnLayoutChangeListener, OnApplyWindowInsetsListener {
    private var top = -1
    private var bottom = -1
    private var system: WindowInsetsCompat? = null

    override fun onApplyWindowInsets(view: View, insets: WindowInsetsCompat): WindowInsetsCompat {
        system = insets
        val sides = insets.horizontalInsets()
        view.updatePadding(left = sides.left, right = sides.right)
        if (!binding.topChrome.isVisible) return insets
        return insets.underChrome(top = binding.topChrome.height, bottom = binding.bottomChrome.height)
    }

    override fun onLayoutChange(view: View, l: Int, t: Int, r: Int, b: Int, ol: Int, ot: Int, or: Int, ob: Int) {
        val nextTop = binding.topChrome.height
        val nextBottom = binding.bottomChrome.height
        if (nextTop == top && nextBottom == bottom) return
        top = nextTop
        bottom = nextBottom
        redispatch()
    }

    fun redispatch() {
        val last = system ?: return
        binding.fragmentContainer.post { ViewCompat.dispatchApplyWindowInsets(binding.fragmentContainer, last) }
    }
}
