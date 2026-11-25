package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.content.ContextWrapper
import android.graphics.Canvas
import android.view.View
import androidx.core.graphics.withTranslation
import androidx.core.view.isVisible

fun interface GlassEdgeOwner {
    fun coverEdge(cover: Any, covered: Boolean)
}

internal class EdgeCovers(private val onChange: () -> Unit) : GlassEdgeOwner {
    private val covers = mutableSetOf<Any>()

    val covered: Boolean get() = covers.isNotEmpty()

    override fun coverEdge(cover: Any, covered: Boolean) {
        val changed = if (covered) covers.add(cover) else covers.remove(cover)
        if (changed) onChange()
    }
}

fun View.glassEdgeAbove(): GlassEdgeOwner? {
    val header = generateSequence(parent) { it.parent }.filterIsInstance<GlassHeaderLayout>().firstOrNull()
    return header?.edgeOwner ?: context.glassEdgeHost()
}

internal fun Context.glassEdgeHost(): GlassEdgeOwner? = generateSequence(this) { (it as? ContextWrapper)?.baseContext }
    .filterIsInstance<GlassEdgeOwner>()
    .firstOrNull()

internal fun View.backdropOf(content: View): GlassSource {
    val origin = IntArray(2)
    val target = IntArray(2)
    return GlassSource { canvas ->
        getLocationInWindow(origin)
        content.getLocationInWindow(target)
        canvas.withTranslation((target[0] - origin[0]).toFloat(), (target[1] - origin[1]).toFloat()) { content.draw(this) }
    }
}

internal fun Canvas.drawSiblingsBelow(view: View) {
    val parent = view.parent as? android.view.ViewGroup ?: return
    (0 until parent.indexOfChild(view)).map(parent::getChildAt).filter { it.isVisible }.forEach { sibling ->
        withTranslation(sibling.x - view.x, sibling.y - view.y) { sibling.draw(this) }
    }
}
