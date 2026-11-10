package dev.proflyder.home.feature.control.presentation

import android.content.Context
import android.view.KeyEvent
import android.view.LayoutInflater
import com.google.android.material.bottomsheet.BottomSheetDialog
import com.google.android.material.slider.Slider
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.SheetVolumeBinding
import kotlin.math.roundToInt

/**
 * Bottom sheet of the station volume: the main level with a mute toggle and the
 * announcement level. A command is sent when a slider is released, not while
 * dragging. While the sheet is open the phone's hardware volume keys drive the
 * station, never the phone's own volume.
 */
internal class VolumeSheet(
    context: Context,
    private val entry: ControlEntry,
    private val announceEntry: ControlEntry?,
    private val onLevel: (ControlEntry, Int) -> Unit,
    private val onMuted: (ControlEntry, Boolean) -> Unit,
) {
    private val dialog = BottomSheetDialog(context)
    private val views = SheetVolumeBinding.inflate(LayoutInflater.from(context))
    private var level = snap(entry.volume?.level ?: 0)
    private var muted = entry.volume?.muted ?: false
    private val announceLevel = snap(announceEntry?.volume?.level ?: 0)

    init {
        dialog.setContentView(views.root)
        views.volumeTitle.text = views.root.context.getString(R.string.control_volume_title)
        setupMain()
        setupAnnounce()
        dialog.setOnKeyListener { _, keyCode, event -> onHardwareKey(keyCode, event) }
    }

    fun show() = dialog.show()

    fun dismiss() = dialog.dismiss()

    private fun setupMain() {
        views.volumeMainSlider.value = level.coerceIn(0, 100).toFloat()
        showMainValue()
        syncMute()
        views.volumeMainSlider.addOnChangeListener { _, value, fromUser ->
            if (fromUser) showMainPercent(value)
        }
        views.volumeMainSlider.onRelease { sendLevel(entry, it) }
        views.volumeMute.setOnClickListener { toggleMute() }
    }

    private fun setupAnnounce() {
        views.volumeAnnounceGroup.setVisible(announceEntry != null)
        announceEntry ?: return
        views.volumeAnnounceSlider.value = announceLevel.coerceIn(0, 100).toFloat()
        views.volumeAnnounceValue.text = views.root.context.getString(
            R.string.control_volume_announce_level,
            announceLevel,
        )
        views.volumeAnnounceSlider.addOnChangeListener { _, value, _ -> showAnnouncePercent(value) }
        views.volumeAnnounceSlider.onRelease { sendLevel(announceEntry, it) }
    }

    private fun sendLevel(target: ControlEntry, value: Float) {
        val next = value.roundToInt().coerceIn(0, 100)
        onLevel(target, next)
    }

    private fun toggleMute() {
        muted = !muted
        onMuted(entry, muted)
        syncMute()
    }

    private fun syncMute() {
        views.volumeMute.setImageResource(
            if (muted) R.drawable.control_ic_volume_off else R.drawable.control_ic_volume_up,
        )
        views.volumeMute.contentDescription = views.root.context.getString(
            if (muted) R.string.control_volume_unmute else R.string.control_volume_mute,
        )
        showMainValue()
    }

    private fun showMainValue() {
        views.volumeMainValue.text =
            if (muted) views.root.context.getString(R.string.control_volume_muted) else percent(level)
    }

    private fun showMainPercent(value: Float) {
        level = value.roundToInt()
        views.volumeMainValue.text = percent(level)
    }

    private fun showAnnouncePercent(value: Float) {
        val shown = value.roundToInt()
        views.volumeAnnounceValue.text = views.root.context.getString(
            R.string.control_volume_announce_level,
            shown,
        )
    }

    private fun percent(value: Int): String =
        views.root.context.getString(R.string.control_volume_level, value)

    /**
     * Hardware volume keys adjust the station by five percent per press. The
     * event is consumed even at the edge (0 or 100), so the phone's own stream
     * never changes while the sheet is open.
     */
    internal fun onHardwareKey(keyCode: Int, event: KeyEvent): Boolean {
        val step = when (keyCode) {
            KeyEvent.KEYCODE_VOLUME_UP -> STEP
            KeyEvent.KEYCODE_VOLUME_DOWN -> -STEP
            else -> return false
        }
        if (event.action != KeyEvent.ACTION_DOWN) return true
        applyHardwareStep(step)
        return true
    }

    private fun applyHardwareStep(step: Int) {
        level = (level + step).coerceIn(0, 100)
        views.volumeMainSlider.value = level.toFloat()
        showMainValue()
        onLevel(entry, level)
    }

    private fun Slider.onRelease(action: (Float) -> Unit) {
        addOnSliderTouchListener(object : Slider.OnSliderTouchListener {
            override fun onStartTrackingTouch(slider: Slider) = Unit

            override fun onStopTrackingTouch(slider: Slider) = action(slider.value)
        })
    }

    private companion object {
        const val STEP = 5

        fun snap(value: Int): Int = (value / STEP.toDouble()).roundToInt().coerceIn(0, 20) * STEP
    }
}
