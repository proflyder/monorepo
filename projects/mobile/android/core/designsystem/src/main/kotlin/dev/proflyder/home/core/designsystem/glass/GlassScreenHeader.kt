package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.graphics.Canvas
import android.util.AttributeSet
import android.view.ViewGroup
import android.widget.LinearLayout
import androidx.core.graphics.withTranslation
import androidx.core.view.isVisible

class GlassScreenHeader @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : LinearLayout(context, attrs) {
    private val surface = GlassSurface(context)
    private val chromeListener = GlassChromeListener(::follow)
    private var chrome: GlassChrome? = null

    init {
        orientation = VERTICAL
        setWillNotDraw(false)
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        chrome = context.glassChrome()?.also { it.addListener(chromeListener) }
    }

    override fun onDetachedFromWindow() {
        chrome?.let {
            it.removeListener(chromeListener)
            it.detachHeader(this)
        }
        chrome = null
        super.onDetachedFromWindow()
    }

    override fun onLayout(changed: Boolean, l: Int, t: Int, r: Int, b: Int) {
        super.onLayout(changed, l, t, r, b)
        chrome?.attachHeader(this, height)
    }

    override fun draw(canvas: Canvas) {
        surface.draw(canvas, width, height) { recording -> drawContentBehind(recording) }
        super.draw(canvas)
    }

    private fun follow(state: GlassChrome) {
        val target = (state.barHeight - state.headerHidden).toFloat()
        if (translationY != target) translationY = target
        invalidate()
    }

    private fun drawContentBehind(canvas: Canvas) {
        val parent = parent as? ViewGroup ?: return
        val index = parent.indexOfChild(this)
        (0 until index).map(parent::getChildAt).filter { it.isVisible }.forEach { sibling ->
            canvas.withTranslation(sibling.x - x, sibling.y - y) { sibling.draw(this) }
        }
    }
}
