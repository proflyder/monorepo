package dev.proflyder.home.feature.control.presentation

import android.content.Context
import android.content.res.ColorStateList
import android.graphics.drawable.GradientDrawable
import android.view.LayoutInflater
import com.google.android.material.bottomsheet.BottomSheetDialog
import com.google.android.material.slider.Slider
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.LightAdjust
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.control.LightColors
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.SheetLightBinding
import kotlin.math.roundToInt

/**
 * Bottom sheet that adjusts one light. Only the controls the light actually
 * supports are shown (a white-only bulb has no color; a dimmable bulb has just
 * brightness). A command is sent when a slider is released, not while dragging,
 * so one gesture is one command.
 */
internal class LightSheet(
    context: Context,
    entry: ControlEntry,
    private val onAdjust: (LightAdjust) -> Unit,
) {
    private val dialog = BottomSheetDialog(context)
    private val views = SheetLightBinding.inflate(LayoutInflater.from(context))
    private val light = requireNotNull(entry.light)

    private var temperature = light.kelvin
    private var hue = light.hue
    private var saturation = light.saturation
    /** Which of temperature / color the preview currently shows. */
    private var colorMode = light.color && (light.mode == "color" || (light.mode.isEmpty() && !light.colorTemp))

    /** Values Home Assistant does not report (lamp off, or the other mode is active) show "—", not a guess. */
    private var brightnessKnown = light.brightnessPct > 0
    private var temperatureKnown = light.kelvin > 0

    init {
        dialog.setContentView(views.root)
        views.lightTitle.text = entry.title
        setupBrightness()
        setupTemperature()
        setupColor()
        renderPreview()
    }

    fun show() = dialog.show()

    fun dismiss() = dialog.dismiss()

    private fun setupBrightness() {
        views.lightBrightnessGroup.setVisible(light.brightness)
        val start = if (brightnessKnown) light.brightnessPct.coerceIn(1, 100).toFloat() else 100f
        views.lightBrightness.value = start
        showPercent(start)
        views.lightBrightness.addOnChangeListener { _, value, fromUser ->
            if (fromUser) brightnessKnown = true
            showPercent(value)
        }
        views.lightBrightness.onRelease { onAdjust(LightAdjust(brightnessPct = it.roundToInt())) }
    }

    private fun showPercent(value: Float) {
        views.lightBrightnessValue.text =
            if (brightnessKnown) views.root.context.getString(R.string.control_light_percent, value.roundToInt()) else UNKNOWN
    }

    private fun setupTemperature() {
        views.lightTempGroup.setVisible(light.colorTemp)
        if (!light.colorTemp) return
        views.lightTemp.valueFrom = roundTo50(light.minKelvin).toFloat()
        views.lightTemp.valueTo = roundTo50(light.maxKelvin).coerceAtLeast(roundTo50(light.minKelvin) + 50).toFloat()
        val start = (if (temperature > 0) temperature else (light.minKelvin + light.maxKelvin) / 2)
        temperature = roundTo50(start).coerceIn(views.lightTemp.valueFrom.toInt(), views.lightTemp.valueTo.toInt())
        views.lightTempBar.background = GradientDrawable(
            GradientDrawable.Orientation.LEFT_RIGHT,
            intArrayOf(
                LightColors.kelvinToArgb(views.lightTemp.valueFrom.toInt()),
                LightColors.kelvinToArgb(views.lightTemp.valueTo.toInt()),
            ),
        ).apply { cornerRadius = 100f }
        views.lightTemp.value = temperature.toFloat()
        showKelvin()
        views.lightTemp.addOnChangeListener { _, value, fromUser ->
            temperature = value.roundToInt()
            if (fromUser) {
                colorMode = false
                temperatureKnown = true
            }
            showKelvin()
            renderPreview()
        }
        views.lightTemp.onRelease { onAdjust(LightAdjust(kelvin = it.roundToInt())) }
    }

    private fun showKelvin() {
        views.lightTempValue.text =
            if (temperatureKnown) views.root.context.getString(R.string.control_light_kelvin, temperature) else UNKNOWN
    }

    private fun setupColor() {
        views.lightColorGroup.setVisible(light.color)
        if (!light.color) return
        views.lightHueBar.background = GradientDrawable(
            GradientDrawable.Orientation.LEFT_RIGHT,
            IntArray(7) { LightColors.hsToArgb(it * 60, 100) },
        ).apply { cornerRadius = 100f }
        views.lightHue.value = hue.coerceIn(0, 360).toFloat()
        views.lightSaturation.value = saturation.coerceIn(0, 100).toFloat()
        showSaturation()
        views.lightHue.addOnChangeListener { _, value, fromUser ->
            hue = value.roundToInt()
            if (fromUser) colorMode = true
            renderPreview()
        }
        views.lightSaturation.addOnChangeListener { _, value, fromUser ->
            saturation = value.roundToInt()
            if (fromUser) colorMode = true
            showSaturation()
            renderPreview()
        }
        // Hue and saturation are always sent together: the server needs both.
        val sendColor: (Float) -> Unit = { onAdjust(LightAdjust(hue = hue, saturation = saturation)) }
        views.lightHue.onRelease(sendColor)
        views.lightSaturation.onRelease(sendColor)
    }

    private fun showSaturation() {
        views.lightSaturationValue.text = views.root.context.getString(R.string.control_light_percent, saturation)
    }

    private fun renderPreview() {
        val argb = when {
            colorMode && light.color -> LightColors.hsToArgb(hue, saturation)
            light.colorTemp && temperatureKnown -> LightColors.kelvinToArgb(temperature)
            else -> LightColors.kelvinToArgb(NEUTRAL_KELVIN)
        }
        (views.lightPreview.background.mutate() as GradientDrawable).setColor(ColorStateList.valueOf(argb))
    }

    private fun Slider.onRelease(action: (Float) -> Unit) {
        addOnSliderTouchListener(object : Slider.OnSliderTouchListener {
            override fun onStartTrackingTouch(slider: Slider) = Unit
            override fun onStopTrackingTouch(slider: Slider) = action(slider.value)
        })
    }

    private fun roundTo50(value: Int) = (value / 50.0).roundToInt() * 50

    private companion object {
        const val NEUTRAL_KELVIN = 4000
        const val UNKNOWN = "—"
    }
}
