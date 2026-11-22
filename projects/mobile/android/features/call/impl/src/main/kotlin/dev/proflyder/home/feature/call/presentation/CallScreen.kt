package dev.proflyder.home.feature.call.presentation

import android.view.View
import dev.proflyder.home.feature.call.impl.R
import dev.proflyder.home.feature.call.impl.databinding.ActivityCallBinding

internal class CallScreen(private val binding: ActivityCallBinding, private val texts: CallTexts) {
    fun render(state: CallScreenState) {
        binding.callStatus.text = texts.status(state.status)
        val failure = (state.status as? CallStatus.Ended)?.failure
        binding.callError.text = failure?.let(texts::failure)
        binding.callError.visibility = if (failure == null) View.GONE else View.VISIBLE
        binding.callMute.isEnabled = state.live
        binding.callRoute.isEnabled = state.live
        binding.callMute.setText(if (state.muted) R.string.call_unmute else R.string.call_mute)
        binding.callMute.setIconResource(if (state.muted) R.drawable.call_ic_mic_off else R.drawable.call_ic_mic)
        binding.callHangup.setText(if (state.status is CallStatus.Ended) R.string.call_close else R.string.call_hangup)
    }
}
