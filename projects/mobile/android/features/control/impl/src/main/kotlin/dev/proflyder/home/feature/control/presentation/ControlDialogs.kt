package dev.proflyder.home.feature.control.presentation

import android.content.Context
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.feature.control.impl.R

internal class ControlDialogs(
    private val context: () -> Context,
    private val dispatch: (ControlIntent) -> Unit,
) {
    private var lightSheet: LightSheet? = null
    private var volumeSheet: VolumeSheet? = null
    private var speakDialog: SpeakDialog? = null

    fun openLight(entry: ControlEntry) {
        lightSheet?.dismiss()
        lightSheet = LightSheet(context(), entry) { command(DeviceCommand.Adjust(entry, it)) }.also { it.show() }
    }

    fun openVolume(entry: ControlEntry, announce: ControlEntry?) {
        volumeSheet?.dismiss()
        volumeSheet = VolumeSheet(
            context = context(),
            entry = entry,
            announceEntry = announce,
            onLevel = { target, level -> command(DeviceCommand.Volume(target, level)) },
            onMuted = { target, muted -> command(DeviceCommand.Mute(target, muted)) },
        ).also { it.show() }
    }

    fun openSpeak(entry: ControlEntry) {
        speakDialog?.dismiss()
        speakDialog = SpeakDialog(context()) { command(DeviceCommand.Speak(entry, it)) }.also { it.show() }
    }

    fun confirmPin(entry: ControlEntry, pinned: Boolean) {
        MaterialAlertDialogBuilder(context())
            .setMessage(entry.localizedDisplayName(context()))
            .setNegativeButton(android.R.string.cancel, null)
            .setPositiveButton(if (pinned) R.string.control_unpin else R.string.control_pin) { _, _ -> pin(entry) }
            .show()
    }

    fun dismissAll() {
        lightSheet?.dismiss()
        lightSheet = null
        volumeSheet?.dismiss()
        volumeSheet = null
        speakDialog?.dismiss()
        speakDialog = null
    }

    private fun pin(entry: ControlEntry) = dispatch(ControlIntent.TogglePin(PinSubject.Entry(entry, entry.devicePin)))

    private fun command(command: DeviceCommand) = dispatch(ControlIntent.Command(command))
}
