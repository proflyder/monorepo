package dev.proflyder.home.core.designsystem.glass

import android.content.Context
import android.util.AttributeSet
import android.widget.LinearLayout
import androidx.core.view.children
import androidx.core.view.isVisible
import com.google.android.material.floatingactionbutton.FloatingActionButton

class FloatingGlassFooter @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : LinearLayout(context, attrs) {
    init {
        orientation = VERTICAL
    }

    val scrimTop: Float
        get() {
            val surface = children.firstOrNull { it.isVisible && it !is FloatingActionButton }
            return top + translationY + (surface?.top ?: height)
        }
}
