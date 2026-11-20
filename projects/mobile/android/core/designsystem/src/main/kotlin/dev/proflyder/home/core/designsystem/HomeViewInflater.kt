package dev.proflyder.home.core.designsystem

import android.content.Context
import android.util.AttributeSet
import android.view.View
import com.google.android.material.textfield.TextInputLayout
import com.google.android.material.theme.MaterialComponentsViewInflater

class HomeViewInflater : MaterialComponentsViewInflater() {
    override fun createView(context: Context, name: String, attrs: AttributeSet): View? = when (name) {
        TEXT_INPUT_LAYOUT -> CardTextInputLayout(context, attrs)
        else -> null
    }

    private companion object {
        val TEXT_INPUT_LAYOUT: String = TextInputLayout::class.java.name
    }
}
