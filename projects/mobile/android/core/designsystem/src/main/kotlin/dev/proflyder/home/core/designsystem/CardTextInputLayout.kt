package dev.proflyder.home.core.designsystem

import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Rect
import android.util.AttributeSet
import android.view.ViewTreeObserver
import android.widget.EditText
import androidx.core.content.ContextCompat
import androidx.core.view.doOnAttach
import androidx.core.view.doOnDetach
import com.google.android.material.shape.MaterialShapeDrawable
import com.google.android.material.textfield.TextInputLayout

class CardTextInputLayout @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = com.google.android.material.R.attr.textInputStyle,
) : TextInputLayout(context, attrs, defStyleAttr) {
    private val outline = MaterialShapeDrawable(shapeAppearanceModel).apply {
        fillColor = ColorStateList.valueOf(Color.TRANSPARENT)
    }
    private val boxBounds = Rect()
    private val restWidth = resources.getDimensionPixelSize(R.dimen.ds_stroke_thin)
    private val focusWidth = resources.getDimensionPixelSize(R.dimen.ds_stroke_thick)
    private val strokeStates: ColorStateList = checkNotNull(ContextCompat.getColorStateList(context, R.color.ds_text_input_stroke))
    private val focusWatcher = ViewTreeObserver.OnGlobalFocusChangeListener { _, _ -> invalidate() }

    init {
        setWillNotDraw(false)
        doOnAttach { viewTreeObserver.addOnGlobalFocusChangeListener(focusWatcher) }
        doOnDetach { viewTreeObserver.removeOnGlobalFocusChangeListener(focusWatcher) }
    }

    val outlineWidth: Int
        get() = when {
            error != null || isFocusedField() -> focusWidth
            else -> restWidth
        }

    val outlineColor: Int
        get() = when {
            error != null -> boxStrokeErrorColor?.defaultColor ?: boxStrokeColor
            else -> restingColor()
        }

    override fun setError(errorText: CharSequence?) {
        super.setError(errorText)
        invalidate()
    }

    override fun draw(canvas: Canvas) {
        super.draw(canvas)
        val field = editText
        when {
            field != null && boxStrokeWidth == 0 && boxBackgroundMode == BOX_BACKGROUND_FILLED -> drawOutline(canvas, field)
        }
    }

    private fun drawOutline(canvas: Canvas, field: EditText) {
        field.getDrawingRect(boxBounds)
        offsetDescendantRectToMyCoords(field, boxBounds)
        outline.shapeAppearanceModel = shapeAppearanceModel
        outline.setStroke(outlineWidth.toFloat(), outlineColor)
        outline.bounds = boxBounds
        outline.draw(canvas)
    }

    private fun isFocusedField(): Boolean = editText?.hasFocus() == true

    private fun restingColor(): Int = strokeStates.getColorForState(
        when {
            !isEnabled -> intArrayOf(-android.R.attr.state_enabled)
            isFocusedField() -> intArrayOf(android.R.attr.state_focused, android.R.attr.state_enabled)
            else -> intArrayOf(android.R.attr.state_enabled)
        },
        strokeStates.defaultColor,
    )
}
