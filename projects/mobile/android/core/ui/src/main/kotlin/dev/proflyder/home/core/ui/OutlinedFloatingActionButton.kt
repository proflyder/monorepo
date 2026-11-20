package dev.proflyder.home.core.ui

import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Rect
import android.util.AttributeSet
import androidx.core.content.withStyledAttributes
import com.google.android.material.floatingactionbutton.FloatingActionButton
import com.google.android.material.shape.MaterialShapeDrawable
import com.google.android.material.shape.ShapeAppearanceModel

class OutlinedFloatingActionButton @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = com.google.android.material.R.attr.floatingActionButtonStyle,
) : FloatingActionButton(context, attrs, defStyleAttr) {
    private val outline = MaterialShapeDrawable(shapeAppearanceModel).apply {
        fillColor = ColorStateList.valueOf(Color.TRANSPARENT)
    }
    private val contentBounds = Rect()

    var strokeWidth: Int = 0
        set(value) {
            field = value
            outline.strokeWidth = value.toFloat()
            invalidate()
        }

    var strokeColor: ColorStateList? = null
        set(value) {
            field = value
            outline.strokeColor = value
            invalidate()
        }

    init {
        context.withStyledAttributes(attrs, R.styleable.OutlinedFloatingActionButton, defStyleAttr, 0) {
            strokeWidth = getDimensionPixelSize(R.styleable.OutlinedFloatingActionButton_strokeWidth, 0)
            strokeColor = getColorStateList(R.styleable.OutlinedFloatingActionButton_strokeColor)
        }
    }

    override fun setShapeAppearanceModel(shapeAppearance: ShapeAppearanceModel) {
        super.setShapeAppearanceModel(shapeAppearance)
        outline.shapeAppearanceModel = shapeAppearance
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        if (strokeWidth <= 0) return
        getMeasuredContentRect(contentBounds)
        contentBounds.offset(-scrollX, -scrollY)
        outline.bounds = contentBounds
        outline.state = drawableState
        outline.draw(canvas)
    }
}
