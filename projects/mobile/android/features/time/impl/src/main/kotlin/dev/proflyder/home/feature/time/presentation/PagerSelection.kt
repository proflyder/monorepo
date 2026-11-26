package dev.proflyder.home.feature.time.presentation

import androidx.viewpager2.widget.ViewPager2

internal fun ViewPager2.showPage(index: Int, animate: Boolean) {
    if (currentItem == index) return
    setCurrentItem(index, animate && isLaidOut)
}
