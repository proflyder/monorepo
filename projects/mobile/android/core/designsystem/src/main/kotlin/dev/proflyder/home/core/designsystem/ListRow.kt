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
import com.google.android.material.materialswitch.MaterialSwitch

class ListRow @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : LinearLayout(context, attrs) {
    private val iconView: ImageView
    private val titleView: TextView
    private val subtitleView: TextView
    private val valueView: TextView
    private val chevronView: ImageView

    val switch: MaterialSwitch

    init {
        orientation = HORIZONTAL
        gravity = Gravity.CENTER_VERTICAL
        minimumHeight = resources.getDimensionPixelSize(R.dimen.ds_list_row_min_height)
        LayoutInflater.from(context).inflate(R.layout.ds_list_row, this)
        iconView = findViewById(R.id.ds_row_icon)
        titleView = findViewById(R.id.ds_row_title)
        subtitleView = findViewById(R.id.ds_row_subtitle)
        valueView = findViewById(R.id.ds_row_value)
        chevronView = findViewById(R.id.ds_row_chevron)
        switch = findViewById(R.id.ds_row_switch)
        context.withStyledAttributes(attrs, R.styleable.ListRow) { readAttributes(this) }
    }

    var icon: Drawable?
        get() = iconView.drawable
        set(value) {
            iconView.setImageDrawable(value)
            iconView.isVisible = value != null
        }

    var title: CharSequence?
        get() = titleView.text
        set(value) {
            titleView.text = value
        }

    var subtitle: CharSequence?
        get() = subtitleView.text
        set(value) = showText(subtitleView, value)

    var value: CharSequence?
        get() = valueView.text
        set(value) = showText(valueView, value)

    var chevron: Boolean
        get() = chevronView.isVisible
        set(value) {
            chevronView.isVisible = value
        }

    var hasSwitch: Boolean
        get() = switch.isVisible
        set(value) {
            switch.isVisible = value
        }

    private fun readAttributes(array: TypedArray) {
        icon = array.getDrawable(R.styleable.ListRow_rowIcon)
        title = array.getText(R.styleable.ListRow_rowTitle)
        subtitle = array.getText(R.styleable.ListRow_rowSubtitle)
        value = array.getText(R.styleable.ListRow_rowValue)
        chevron = array.getBoolean(R.styleable.ListRow_rowChevron, false)
        hasSwitch = array.getBoolean(R.styleable.ListRow_rowSwitch, false)
    }

    private fun showText(view: TextView, text: CharSequence?) {
        view.text = text
        view.isVisible = !text.isNullOrEmpty()
    }
}
