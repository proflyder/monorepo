package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.util.AttributeSet
import android.widget.LinearLayout
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.updatePadding

class GlassScreenHeader @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : LinearLayout(context, attrs) {
    private val baseTop = paddingTop

    init {
        orientation = VERTICAL
        ViewCompat.setOnApplyWindowInsetsListener(this) { view, insets ->
            view.updatePadding(top = baseTop + insets.getInsets(WindowInsetsCompat.Type.systemBars()).top)
            WindowInsetsCompat.CONSUMED
        }
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        context.glassBlockHost()?.attachBlock(this)
    }

    override fun onDetachedFromWindow() {
        context.glassBlockHost()?.detachBlock(this)
        super.onDetachedFromWindow()
    }
}
