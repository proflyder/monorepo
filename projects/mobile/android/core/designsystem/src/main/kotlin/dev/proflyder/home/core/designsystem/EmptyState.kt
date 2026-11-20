package dev.proflyder.home.core.designsystem

import android.content.Context
import android.content.res.TypedArray
import android.graphics.drawable.Drawable
import android.util.AttributeSet
import android.view.Gravity
import android.view.LayoutInflater
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.TextView
import androidx.core.content.withStyledAttributes
import androidx.core.view.isVisible
import com.google.android.material.button.MaterialButton

class EmptyState @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : LinearLayout(context, attrs) {
    private val iconView: ImageView
    private val textView: TextView

    val action: MaterialButton

    init {
        orientation = VERTICAL
        gravity = Gravity.CENTER
        val padding = resources.getDimensionPixelSize(R.dimen.ds_space_xl)
        setPadding(padding, padding, padding, padding)
        LayoutInflater.from(context).inflate(R.layout.ds_empty_state, this)
        iconView = findViewById(R.id.ds_empty_icon)
        textView = findViewById(R.id.ds_empty_text)
        action = findViewById(R.id.ds_empty_action)
        context.withStyledAttributes(attrs, R.styleable.EmptyState) { readAttributes(this) }
    }

    var icon: Drawable?
        get() = iconView.drawable
        set(value) {
            iconView.setImageDrawable(value)
            iconView.isVisible = value != null
        }

    var text: CharSequence?
        get() = textView.text
        set(value) {
            textView.text = value
        }

    var actionText: CharSequence?
        get() = action.text
        set(value) {
            action.text = value
            action.isVisible = !value.isNullOrEmpty()
        }

    private fun readAttributes(array: TypedArray) {
        icon = array.getDrawable(R.styleable.EmptyState_emptyIcon)
        text = array.getText(R.styleable.EmptyState_emptyText)
        actionText = array.getText(R.styleable.EmptyState_emptyAction)
    }
}
