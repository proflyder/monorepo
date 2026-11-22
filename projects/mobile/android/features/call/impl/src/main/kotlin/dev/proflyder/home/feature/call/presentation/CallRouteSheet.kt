package dev.proflyder.home.feature.call.presentation

import android.content.Context
import android.view.LayoutInflater
import android.widget.LinearLayout
import com.google.android.material.bottomsheet.BottomSheetDialog
import dev.proflyder.home.core.designsystem.Components
import dev.proflyder.home.feature.call.audio.AudioRouteOption
import dev.proflyder.home.feature.call.audio.CallAudioRouteState
import dev.proflyder.home.feature.call.impl.R
import dev.proflyder.home.feature.call.impl.databinding.SheetCallRoutesBinding

internal class CallRouteSheet(
    private val context: Context,
    private val texts: CallTexts,
    private val onIntent: (CallIntent) -> Unit,
) {
    private var dialog: BottomSheetDialog? = null
    private var list: LinearLayout? = null

    fun render(visible: Boolean, routes: CallAudioRouteState) {
        if (!visible) {
            dismiss()
            return
        }
        val container = list ?: open()
        container.removeAllViews()
        routes.available.forEach { container.addView(button(container, it, it.route == routes.selected), params()) }
    }

    fun dismiss() {
        dialog?.setOnDismissListener(null)
        dialog?.dismiss()
        dialog = null
        list = null
    }

    private fun open(): LinearLayout {
        val binding = SheetCallRoutesBinding.inflate(LayoutInflater.from(context))
        val sheet = BottomSheetDialog(context)
        sheet.setContentView(binding.root)
        sheet.setOnDismissListener { onIntent(CallIntent.HideRoutes) }
        sheet.show()
        dialog = sheet
        list = binding.callRouteList
        return binding.callRouteList
    }

    private fun button(container: LinearLayout, option: AudioRouteOption, selected: Boolean) =
        Components.tonalButtonL(container).apply {
            text = texts.route(option, selected)
            setOnClickListener { onIntent(CallIntent.SelectRoute(option.route)) }
        }

    private fun params(): LinearLayout.LayoutParams =
        LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT)
            .apply { topMargin = context.resources.getDimensionPixelSize(R.dimen.call_route_gap) }
}
