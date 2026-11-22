package dev.proflyder.home

import androidx.core.view.ViewCompat
import androidx.core.view.updatePadding
import dev.proflyder.home.core.ui.applyNavigationInset
import dev.proflyder.home.core.ui.doOnApplyInsets
import dev.proflyder.home.core.ui.forContentBetweenBars
import dev.proflyder.home.core.ui.horizontalInsets
import dev.proflyder.home.core.ui.keyboardVisible
import dev.proflyder.home.core.ui.topInset
import dev.proflyder.home.databinding.ActivityMainBinding

internal fun ActivityMainBinding.applySystemBarInsets(tabs: BottomTabs) {
    appbar.doOnApplyInsets { view, insets, initial ->
        val sides = insets.horizontalInsets()
        view.updatePadding(left = sides.left, top = initial.top + insets.topInset(), right = sides.right)
    }
    ViewCompat.setOnApplyWindowInsetsListener(root) { _, insets -> insets.also { tabs.keyboard(it.keyboardVisible()) } }
    homeNoticeAnchor.applyNavigationInset()
    ViewCompat.setOnApplyWindowInsetsListener(fragmentContainer) { view, insets ->
        val sides = insets.horizontalInsets()
        view.updatePadding(left = sides.left, right = sides.right)
        insets.forContentBetweenBars(bottomCovered = tabs.coversContent)
    }
}
