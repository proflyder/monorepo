package dev.proflyder.home.core.presentation

import android.widget.CompoundButton

interface ControlBinder<T> {
    fun render(value: T)
}

class SwitchBinder(
    private val view: CompoundButton,
    private val onIntent: (Boolean) -> Unit,
) : ControlBinder<Boolean> {
    private var rendered: Boolean = view.isChecked

    init {
        view.setOnClickListener { onClicked() }
    }

    override fun render(value: Boolean) {
        rendered = value
        if (view.isChecked != value) view.isChecked = value
    }

    private fun onClicked() {
        val wanted = view.isChecked
        view.isChecked = rendered
        view.jumpDrawablesToCurrentState()
        onIntent(wanted)
    }
}
