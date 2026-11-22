package dev.proflyder.home.core.designsystem

import android.content.Context
import android.view.LayoutInflater
import android.view.View
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.TextView
import androidx.core.view.isVisible
import androidx.core.widget.doAfterTextChanged
import com.google.android.material.chip.Chip
import com.google.android.material.chip.ChipGroup
import com.google.android.material.slider.Slider
import dev.proflyder.home.core.model.LightParams
import dev.proflyder.home.core.model.LightRanges
import kotlin.math.roundToInt

data class LightParamsEditorState(
    val ranges: LightRanges?,
    val effects: List<String>,
    val params: LightParams,
    val transitionText: String,
)

sealed interface LightParamsEdit {
    data class Brightness(val percent: Int) : LightParamsEdit

    data class Kelvin(val kelvin: Int) : LightParamsEdit

    data class Effect(val name: String?) : LightParamsEdit

    data class Transition(val text: String) : LightParamsEdit
}

fun LightParamsEditorState.apply(edit: LightParamsEdit): LightParamsEditorState = when (edit) {
    is LightParamsEdit.Brightness -> copy(params = params.copy(brightnessPct = edit.percent))
    is LightParamsEdit.Kelvin -> copy(params = params.copy(kelvin = LightRanges.roundKelvin(edit.kelvin)))
    is LightParamsEdit.Effect -> copy(params = params.copy(effect = edit.name))
    is LightParamsEdit.Transition -> copy(
        transitionText = edit.text,
        params = params.copy(transitionSeconds = LightRanges.transitionOf(edit.text)),
    )
}

val LightParamsEditorState.isValid: Boolean
    get() = transitionText.isBlank() || LightRanges.transitionOf(transitionText) != null

class LightParamsEditor(
    context: Context,
    private val onEdit: (LightParamsEdit) -> Unit,
) {
    val root: View = LayoutInflater.from(context).inflate(R.layout.ds_light_params_editor, FrameLayout(context), false)

    private val brightnessGroup: View = root.findViewById(R.id.ds_light_brightness_group)
    private val brightness: Slider = root.findViewById(R.id.ds_light_brightness)
    private val brightnessValue: TextView = root.findViewById(R.id.ds_light_brightness_value)
    private val temperatureGroup: View = root.findViewById(R.id.ds_light_temp_group)
    private val temperature: Slider = root.findViewById(R.id.ds_light_temp)
    private val temperatureValue: TextView = root.findViewById(R.id.ds_light_temp_value)
    private val effectLabel: View = root.findViewById(R.id.ds_light_effect_label)
    private val effectChips: ChipGroup = root.findViewById(R.id.ds_light_effects)
    private val transition: EditText = root.findViewById(R.id.ds_light_transition)
    private var renderedEffects: List<String>? = null
    private var renderedEffect: String? = null

    init {
        brightness.addOnChangeListener { _, value, fromUser -> if (fromUser) onEdit(LightParamsEdit.Brightness(value.roundToInt())) }
        temperature.addOnChangeListener { _, value, fromUser -> if (fromUser) onEdit(LightParamsEdit.Kelvin(value.roundToInt())) }
        transition.doAfterTextChanged { text -> onTransitionTyped(text?.toString().orEmpty()) }
    }

    fun render(state: LightParamsEditorState) {
        val ranges = state.ranges
        renderBrightness(ranges?.brightness, state.params.brightnessPct)
        renderTemperature(ranges?.kelvin, state.params.kelvin)
        renderEffects(state.effects, state.params.effect)
        if (transition.text?.toString() != state.transitionText) transition.setText(state.transitionText)
    }

    private fun onTransitionTyped(text: String) {
        onEdit(LightParamsEdit.Transition(text))
    }

    private fun renderBrightness(range: IntRange?, value: Int?) {
        brightnessGroup.isVisible = range != null && value != null
        if (range == null || value == null) return
        brightness.valueFrom = range.first.toFloat()
        brightness.valueTo = range.last.toFloat()
        brightness.value = value.coerceIn(range).toFloat()
        brightnessValue.text = root.context.getString(R.string.ds_light_percent, value)
    }

    private fun renderTemperature(range: IntRange?, value: Int?) {
        temperatureGroup.isVisible = range != null && value != null
        if (range == null || value == null) return
        temperature.valueFrom = range.first.toFloat()
        temperature.valueTo = range.last.toFloat()
        temperature.value = value.coerceIn(range).toFloat()
        temperatureValue.text = root.context.getString(R.string.ds_light_kelvin, value)
    }

    private fun renderEffects(effects: List<String>, selected: String?) {
        effectLabel.isVisible = effects.isNotEmpty()
        effectChips.isVisible = effects.isNotEmpty()
        if (renderedEffects != effects) rebuildChips(effects)
        renderedEffect = selected
        effectChips.children().forEach { chip -> chip.isChecked = chip.tag == selected }
    }

    private fun rebuildChips(effects: List<String>) {
        renderedEffects = effects
        effectChips.removeAllViews()
        if (effects.isEmpty()) return
        effectChips.addView(effectChip(null, root.context.getString(R.string.ds_light_effect_none)))
        effects.forEach { name -> effectChips.addView(effectChip(name, name)) }
    }

    private fun effectChip(name: String?, label: String): Chip = Components.filterChip(effectChips).apply {
        text = label
        tag = name
        isCheckable = true
        setOnClickListener { onChipClicked(this, name) }
    }

    private fun onChipClicked(chip: Chip, name: String?) {
        chip.isChecked = chip.tag == renderedEffect
        onEdit(LightParamsEdit.Effect(name))
    }

    private fun ChipGroup.children(): List<Chip> = (0 until childCount).mapNotNull { getChildAt(it) as? Chip }
}
