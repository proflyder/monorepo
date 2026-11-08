package dev.proflyder.home.feature.scenarios.presentation

import android.content.Context
import android.content.DialogInterface
import android.view.LayoutInflater
import androidx.core.widget.doAfterTextChanged
import com.google.android.material.chip.Chip
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.LightControls
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.scenarios.impl.R
import dev.proflyder.home.feature.scenarios.impl.databinding.DialogScenarioLightParamsBinding
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import kotlin.math.roundToInt

/**
 * Parameters of a light catalog step: brightness, color temperature, effect and
 * transition. Only controls the light reports are shown; `turn_off` accepts
 * only a transition, matching the executor.
 */
internal class LightParamsDialog(
    context: Context,
    private val entry: ControlEntry,
    private val action: String,
    initialParamsJson: String,
    private val onSave: (String) -> Unit,
) {
    private val views = DialogScenarioLightParamsBinding.inflate(LayoutInflater.from(context))
    private val light: LightControls? = entry.light
    private val initial = parse(initialParamsJson)
    private var effect: String? = initial.stringOrNull("effect")

    private val dialog = MaterialAlertDialogBuilder(context)
        .setTitle(R.string.scenarios_light_dialog_title)
        .setView(views.root)
        .setNegativeButton(android.R.string.cancel, null)
        .setPositiveButton(R.string.scenarios_save) { _, _ -> onSave(paramsJson()) }
        .create()

    init {
        val controls = light
        val effects: List<String>
        if (controls != null && action != "turn_off") {
            views.lightBrightnessGroup.setVisible(controls.brightness)
            views.lightTempGroup.setVisible(controls.colorTemp)
            effects = controls.effects
            setupBrightness(controls)
            setupTemperature(controls)
        } else {
            views.lightBrightnessGroup.setVisible(false)
            views.lightTempGroup.setVisible(false)
            effects = emptyList()
        }
        // A stale effect from an older payload must not be sent if the light no
        // longer reports it; the executor rejects unknown effects.
        if (effect != null && effect !in effects) effect = null
        buildEffectChipsIfAny(context, effects)
        views.lightEffectLabel.setVisible(effects.isNotEmpty())
        views.lightEffects.setVisible(effects.isNotEmpty())
        views.lightTransition.setText(
            initial["transition"]?.jsonPrimitive?.content?.takeIf { it != "null" }.orEmpty(),
        )
        views.lightTransition.doAfterTextChanged { renderPositive() }
    }

    private fun buildEffectChipsIfAny(context: Context, effects: List<String>) {
        if (effects.isNotEmpty()) buildEffectChips(context, effects)
    }

    fun show() {
        dialog.setOnShowListener { renderPositive() }
        dialog.show()
    }

    private fun setupBrightness(light: LightControls) {
        val start = (initial.intOrNull("brightness") ?: light.brightnessPct).coerceIn(1, 100)
        views.lightBrightness.value = start.toFloat()
        showBrightness(start)
        views.lightBrightness.addOnChangeListener { _, value, _ -> showBrightness(value.roundToInt()) }
    }

    private fun showBrightness(value: Int) {
        views.lightBrightnessValue.text =
            views.root.context.getString(R.string.scenarios_light_percent, value)
    }

    private fun setupTemperature(light: LightControls) {
        val min = light.minKelvin.coerceAtLeast(1000)
        val max = light.maxKelvin.coerceAtLeast(min + 50)
        views.lightTemp.valueFrom = roundTo50(min).toFloat()
        views.lightTemp.valueTo = roundTo50(max).toFloat().coerceAtLeast(views.lightTemp.valueFrom + 50)
        val start = (initial.intOrNull("kelvin") ?: light.kelvin.takeIf { it > 0 } ?: (min + max) / 2)
            .coerceIn(views.lightTemp.valueFrom.toInt(), views.lightTemp.valueTo.toInt())
        views.lightTemp.value = roundTo50(start).toFloat()
        showTemperature(views.lightTemp.value.roundToInt())
        views.lightTemp.addOnChangeListener { _, value, _ -> showTemperature(value.roundToInt()) }
    }

    private fun showTemperature(value: Int) {
        views.lightTempValue.text =
            views.root.context.getString(R.string.scenarios_light_kelvin, value)
    }

    private fun buildEffectChips(context: Context, effects: List<String>) {
        val none = Chip(context).apply {
            text = context.getString(R.string.scenarios_light_effect_none)
            isCheckable = true
            isChecked = effect == null
        }
        none.setOnCheckedChangeListener { _, checked -> if (checked) effect = null }
        views.lightEffects.addView(none)
        effects.forEach { name ->
            val chip = Chip(context).apply {
                text = name
                isCheckable = true
                isChecked = effect == name
            }
            chip.setOnCheckedChangeListener { _, checked -> if (checked) effect = name }
            views.lightEffects.addView(chip)
        }
    }

    private fun paramsJson(): String = buildJsonObject {
        val controls = light
        if (controls != null && action != "turn_off") {
            if (controls.brightness) {
                put("brightness", views.lightBrightness.value.roundToInt())
            }
            if (controls.colorTemp) {
                put("kelvin", roundTo50(views.lightTemp.value.roundToInt()))
            }
            effect?.let { put("effect", it) }
        }
        transitionSeconds()?.let { put("transition", it) }
    }.toString()

    private fun renderPositive() {
        val button = runCatching { dialog.getButton(DialogInterface.BUTTON_POSITIVE) }.getOrNull() ?: return
        // An empty field means "omit"; a non-numeric or out-of-range value is invalid.
        val raw = views.lightTransition.text?.toString()?.trim().orEmpty()
        button.isEnabled = raw.isEmpty() || transitionSeconds() != null
    }

    private fun transitionSeconds(): Double? {
        val raw = views.lightTransition.text?.toString()?.trim().orEmpty()
        if (raw.isEmpty()) return null
        val value = raw.toDoubleOrNull() ?: return null
        return value.takeIf { it.isFinite() && it in 0.0..300.0 }
    }

    private fun parse(raw: String): JsonObject = runCatching {
        Json.parseToJsonElement(raw.ifBlank { "{}" }) as? JsonObject ?: JsonObject(emptyMap())
    }.getOrElse { JsonObject(emptyMap()) }

    private fun JsonObject.stringOrNull(name: String): String? =
        this[name]?.jsonPrimitive?.content?.takeIf { it != "null" && it.isNotBlank() }

    private fun JsonObject.intOrNull(name: String): Int? =
        this[name]?.jsonPrimitive?.intOrNull

    private fun roundTo50(value: Int): Int = (value / 50.0).roundToInt() * 50
}
