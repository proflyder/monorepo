package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.util.AttributeSet
import android.view.View
import android.view.WindowInsets
import android.widget.FrameLayout
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.children

class GlassHeaderLayout @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : FrameLayout(context, attrs) {
    private var dispatchedHeight = -1
    private var received: WindowInsets? = null

    init {
        clipChildren = false
    }

    internal val edgeOwner: GlassEdgeOwner? get() = header()

    override fun dispatchApplyWindowInsets(insets: WindowInsets): WindowInsets {
        received = insets
        val compat = WindowInsetsCompat.toWindowInsetsCompat(insets, this)
        val header = header()
        dispatchedHeight = header?.height ?: 0
        val below = header?.let { compat.withTop(it.height) } ?: compat
        children.forEach { child -> ViewCompat.dispatchApplyWindowInsets(child, if (child === header) compat else below) }
        return insets
    }

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        super.onLayout(changed, left, top, right, bottom)
        val height = header()?.height ?: 0
        val last = received ?: return
        if (height != dispatchedHeight) post { dispatchApplyWindowInsets(last) }
    }

    private fun header(): GlassScreenHeader? = children.filterIsInstance<GlassScreenHeader>().firstOrNull { it.visibility != View.GONE }

    private fun WindowInsetsCompat.withTop(top: Int): WindowInsetsCompat {
        val bars = getInsets(WindowInsetsCompat.Type.statusBars())
        val cutout = getInsets(WindowInsetsCompat.Type.displayCutout())
        return WindowInsetsCompat.Builder(this)
            .setInsets(WindowInsetsCompat.Type.statusBars(), Insets.of(bars.left, top, bars.right, bars.bottom))
            .setInsets(WindowInsetsCompat.Type.displayCutout(), Insets.of(cutout.left, 0, cutout.right, cutout.bottom))
            .build()
    }
}
