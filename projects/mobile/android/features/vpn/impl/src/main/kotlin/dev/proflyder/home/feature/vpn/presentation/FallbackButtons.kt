package dev.proflyder.home.feature.vpn.presentation

import android.view.LayoutInflater
import android.widget.LinearLayout
import com.google.android.material.button.MaterialButton
import dev.proflyder.home.core.model.VpnProtocolId
import dev.proflyder.home.core.ui.setVisible
import dev.proflyder.home.feature.vpn.VpnTransportInfo
import dev.proflyder.home.feature.vpn.impl.R

internal class FallbackButtons(
    private val container: LinearLayout,
    private val onPick: (VpnProtocolId) -> Unit,
) {
    private var rendered: List<VpnTransportInfo> = emptyList()

    fun render(options: List<VpnTransportInfo>) {
        container.setVisible(options.isNotEmpty())
        if (options == rendered) return
        rendered = options
        container.removeAllViews()
        options.forEach { container.addView(button(it)) }
    }

    private fun button(option: VpnTransportInfo): MaterialButton {
        val inflater = LayoutInflater.from(container.context)
        val button = inflater.inflate(R.layout.vpn_fallback_button, container, false) as MaterialButton
        val label = container.context.getString(option.labelRes)
        button.text = container.context.getString(R.string.vpn_open_via, label)
        button.setOnClickListener { onPick(option.id) }
        return button
    }
}
