package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.content.ContextWrapper
import android.view.View

interface AppBarHost {
    val glassChrome: GlassChrome
}

fun interface GlassChromeListener {
    fun onGlassChromeChanged(chrome: GlassChrome)
}

class GlassChrome {
    private val listeners = mutableListOf<GlassChromeListener>()
    private var header: View? = null
    var barHeight: Int = 0
        private set
    var headerHeight: Int = 0
        private set
    var headerHidden: Int = 0
        private set
    var pinnedSectionBottom: Int? = null
        private set

    val contentTop: Int get() = barHeight + headerHeight

    val glassBottom: Int get() = contentTop - headerHidden

    val edgeBottom: Int get() = pinnedSectionBottom ?: glassBottom

    fun addListener(listener: GlassChromeListener) {
        listeners.add(listener)
        listener.onGlassChromeChanged(this)
    }

    fun removeListener(listener: GlassChromeListener) {
        listeners.remove(listener)
    }

    fun updateBar(height: Int) {
        if (height == barHeight) return
        barHeight = height
        notifyChanged()
    }

    fun attachHeader(view: View, height: Int) {
        if (header === view && height == headerHeight) return
        header = view
        headerHeight = height
        headerHidden = headerHidden.coerceIn(0, height)
        notifyChanged()
    }

    fun detachHeader(view: View) {
        if (header !== view) return
        header = null
        headerHeight = 0
        headerHidden = 0
        notifyChanged()
    }

    fun contentScrolled(dy: Int, atTop: Boolean) {
        headerHidden = if (atTop) 0 else (headerHidden + dy).coerceIn(0, headerHeight)
        notifyChanged()
    }

    fun pinSection(bottom: Int?) {
        if (bottom == pinnedSectionBottom) return
        pinnedSectionBottom = bottom
        notifyChanged()
    }

    fun notifyChanged() {
        listeners.toList().forEach { it.onGlassChromeChanged(this) }
    }
}

fun Context.glassChrome(): GlassChrome? {
    var current: Context? = this
    while (current is ContextWrapper) {
        if (current is AppBarHost) return current.glassChrome
        current = current.baseContext
    }
    return (current as? AppBarHost)?.glassChrome
}

internal fun View.windowTop(): Int = IntArray(2).also(::getLocationInWindow)[1]
