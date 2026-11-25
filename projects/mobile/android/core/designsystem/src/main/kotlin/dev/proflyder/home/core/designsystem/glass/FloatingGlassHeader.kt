package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.util.AttributeSet
import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.children
import androidx.core.view.updatePadding

class FloatingGlassHeader @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : LinearLayout(context, attrs), GlassHeader {
    private val baseTop = paddingTop

    init {
        orientation = HORIZONTAL
        ViewCompat.setOnApplyWindowInsetsListener(this) { view, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            view.updatePadding(top = baseTop + bars.top)
            WindowInsetsCompat.CONSUMED
        }
    }
}

fun ViewGroup.bindGlassBackdrop(content: View) {
    children.forEach { child ->
        when (child) {
            is GlassPillLayout -> child.bindBackdrop(content)
            is ViewGroup -> child.bindGlassBackdrop(content)
        }
    }
}
