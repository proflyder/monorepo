package dev.proflyder.home.core.presentation

import android.text.Editable
import android.text.TextWatcher
import android.view.View
import android.widget.Checkable
import android.widget.EditText
import android.widget.RadioGroup
import dev.proflyder.home.core.designsystem.PillSegments

class CheckableBinder<V>(
    private val view: V,
    private val onIntent: (Boolean) -> Unit,
) : ControlBinder<Boolean> where V : View, V : Checkable {
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

class RadioGroupBinder<T>(
    private val group: RadioGroup,
    private val idOf: (T) -> Int,
    private val valueOf: (Int) -> T?,
    private val onIntent: (T) -> Unit,
) : ControlBinder<T> {
    private var rendered: Int = group.checkedRadioButtonId
    private var rendering = false

    init {
        group.setOnCheckedChangeListener { _, checkedId -> onChecked(checkedId) }
    }

    override fun render(value: T) {
        show(idOf(value))
    }

    private fun onChecked(checkedId: Int) {
        if (rendering || checkedId == rendered) return
        val wanted = valueOf(checkedId)
        show(rendered)
        wanted?.let(onIntent)
    }

    private fun show(id: Int) {
        rendered = id
        rendering = true
        if (group.checkedRadioButtonId != id) group.check(id)
        rendering = false
    }
}

class TextBinder(
    private val view: EditText,
    private val onIntent: (String) -> Unit,
) : ControlBinder<String> {
    private var rendering = false

    init {
        view.addTextChangedListener(IntentTextWatcher { if (!rendering) onIntent(it) })
    }

    override fun render(value: String) {
        if (view.text?.toString() == value) return
        rendering = true
        view.setText(value)
        view.text?.let { view.setSelection(it.length) }
        rendering = false
    }

    private class IntentTextWatcher(private val onText: (String) -> Unit) : TextWatcher {
        override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) = Unit

        override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) = Unit

        override fun afterTextChanged(s: Editable?) {
            onText(s?.toString().orEmpty())
        }
    }
}

class PillSegmentsBinder<T>(
    private val pills: PillSegments,
    private val values: List<T>,
    private val onIntent: (T) -> Unit,
) : ControlBinder<T> {
    init {
        pills.onSelectionRequested = { index -> values.getOrNull(index)?.let(onIntent) }
    }

    override fun render(value: T) {
        val index = values.indexOf(value)
        if (pills.selectedIndex != index) pills.select(index)
    }
}
