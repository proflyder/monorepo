package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.graphics.Canvas
import android.util.AttributeSet
import android.view.View
import androidx.core.graphics.withClip
import androidx.core.graphics.withTranslation

class GlassBarView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : View(context, attrs) {
    internal val surface = GlassSurface(context)
    private val chromeListener = GlassChromeListener { postInvalidateOnAnimation() }
    private var backdrop: View? = null
    private var chrome: GlassChrome? = null

    init {
        setWillNotDraw(false)
        importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO
    }

    fun bind(content: View, chrome: GlassChrome) {
        backdrop = content
        this.chrome?.removeListener(chromeListener)
        this.chrome = chrome
        if (isAttachedToWindow) chrome.addListener(chromeListener)
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        chrome?.addListener(chromeListener)
    }

    override fun onDetachedFromWindow() {
        chrome?.removeListener(chromeListener)
        super.onDetachedFromWindow()
    }

    override fun onDraw(canvas: Canvas) {
        val state = chrome ?: return
        val content = backdrop ?: return
        if (state.barHeight <= 0) return
        val origin = windowTop()
        canvas.withClip(0, 0, width, state.barHeight) {
            surface.draw(this, width, state.barHeight) { recording -> content.drawShifted(recording, origin) }
        }
        surface.style.drawEdge(canvas, width, state.edgeBottom - origin)
    }

    private fun View.drawShifted(canvas: Canvas, origin: Int) {
        canvas.withTranslation(0f, (windowTop() - origin).toFloat()) { draw(this) }
    }
}
