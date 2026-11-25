package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.content.ContextWrapper
import android.graphics.Canvas
import android.view.View
import androidx.core.graphics.withTranslation

interface GlassBlockHost {
    fun attachBlock(block: View)

    fun detachBlock(block: View)

    fun blockMoved()
}

fun Context.glassBlockHost(): GlassBlockHost? = generateSequence(this) { (it as? ContextWrapper)?.baseContext }
    .filterIsInstance<GlassBlockHost>()
    .firstOrNull()

internal fun View.windowOrigin(into: IntArray): IntArray = into.also(::getLocationInWindow)

internal class ContentBackdrop(private val owner: View, private val content: View) : GlassSource {
    private val origin = IntArray(2)
    private val target = IntArray(2)

    override fun drawBehind(canvas: Canvas) {
        owner.windowOrigin(origin)
        content.windowOrigin(target)
        canvas.withTranslation((target[0] - origin[0]).toFloat(), (target[1] - origin[1]).toFloat()) { content.draw(this) }
    }
}
