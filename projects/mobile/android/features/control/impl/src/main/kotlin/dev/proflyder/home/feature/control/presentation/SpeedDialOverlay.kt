package dev.proflyder.home.feature.control.presentation

import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Color
import android.view.Gravity
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.PopupWindow
import androidx.core.graphics.drawable.toDrawable
import com.google.android.material.animation.AnimationUtils
import com.google.android.material.color.MaterialColors
import com.google.android.material.floatingactionbutton.FloatingActionButton
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.feature.control.QuickTile
import dev.proflyder.home.feature.control.SpeedDialItem
import dev.proflyder.home.feature.control.WakeFeedbackKind
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.ItemControlSpeedDialActionBinding
import dev.proflyder.home.feature.control.impl.databinding.OverlayControlSpeedDialBinding

/**
 * The Home quick-actions speed dial. It lives in its own full-window popup so
 * the scrim also covers the bottom navigation. Items are the same Material FAB
 * component as the root action, in a vertical column above a copy of that FAB,
 * with plain labels to their left. They fly out from under the FAB in a
 * bottom-up cascade; closing reverses the animation and fades the scrim.
 */
internal class SpeedDialOverlay(
    private val anchor: View,
    private val fab: FloatingActionButton,
    private val onClick: (SpeedDialItem) -> Unit,
    private val onLongClick: (SpeedDialItem) -> Unit,
    private val onOpenChanged: (Boolean) -> Unit,
) {
    private val context = anchor.context
    private val content = OverlayControlSpeedDialBinding.inflate(LayoutInflater.from(context))
    private val rows = mutableListOf<Row>()
    private var items: List<SpeedDialItem> = emptyList()
    private val scrimDuration = context.resources.getInteger(R.integer.control_speed_dial_scrim_ms).toLong()
    private val itemDuration = context.resources.getInteger(R.integer.control_speed_dial_item_ms).toLong()
    private val stagger = context.resources.getInteger(R.integer.control_speed_dial_stagger_ms).toLong()
    private val gap = context.resources.getDimensionPixelSize(R.dimen.control_speed_dial_fab_margin)

    private val popup = PopupWindow(
        content.root,
        ViewGroup.LayoutParams.MATCH_PARENT,
        ViewGroup.LayoutParams.MATCH_PARENT,
        true,
    ).apply {
        setBackgroundDrawable(Color.TRANSPARENT.toDrawable())
        isClippingEnabled = false
        elevation = 0f
        setOnDismissListener { dismissed() }
    }

    var isOpen = false
        private set

    init {
        content.controlDialScrim.setOnClickListener { close() }
        content.controlDialFab.setOnClickListener { close() }
    }

    fun render(newItems: List<SpeedDialItem>) {
        val changed = rows.size != newItems.size ||
            rows.zip(newItems).any { (row, item) -> row.key != item.key }
        items = newItems
        when (changed) {
            true -> rebuild(newItems)
            false -> bindRows()
        }
    }

    fun open() {
        if (isOpen) return
        isOpen = true
        content.controlDialScrim.alpha = 0f
        content.controlDialFab.rotation = -90f
        popup.showAtLocation(anchor, Gravity.TOP or Gravity.START, 0, 0)
        onOpenChanged(true)
        content.root.post { beginOpen() }
    }

    private fun beginOpen() {
        positionOverRootFab()
        content.controlDialScrim.animate().alpha(1f).setDuration(scrimDuration).start()
        content.controlDialFab.animate()
            .rotation(0f)
            .setDuration(itemDuration)
            .setInterpolator(AnimationUtils.FAST_OUT_SLOW_IN_INTERPOLATOR)
            .start()
        content.root.post {
            capHeight()
            content.controlDialItems.post { animateIn() }
        }
    }

    fun close() {
        if (!isOpen) return
        isOpen = false
        onOpenChanged(false)
        content.controlDialFab.animate()
            .rotation(-90f)
            .setDuration(itemDuration)
            .setInterpolator(AnimationUtils.FAST_OUT_SLOW_IN_INTERPOLATOR)
            .start()
        rows.forEachIndexed { index, row -> animateOut(index, row) }
        val total = rows.size.coerceAtLeast(1) * stagger + itemDuration
        content.root.postDelayed({ fadeOut() }, total)
    }

    /** Dismiss without animation when the fragment view goes away. */
    fun reset() {
        rows.forEach { it.view.animate().cancel() }
        isOpen = false
        popup.dismiss()
    }

    private fun dismissed() {
        if (isOpen) {
            isOpen = false
            onOpenChanged(false)
        }
        rows.forEach { row ->
            row.view.translationY = 0f
            row.view.alpha = 1f
        }
        content.controlDialScrim.alpha = 0f
        content.controlDialFab.rotation = -90f
    }

    private fun positionOverRootFab() {
        val fabLocation = IntArray(2).also(fab::getLocationOnScreen)
        val rootLocation = IntArray(2).also(content.root::getLocationOnScreen)
        val left = fabLocation[0] - rootLocation[0]
        val top = fabLocation[1] - rootLocation[1]
        content.controlDialFab.layoutParams = FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.WRAP_CONTENT,
            ViewGroup.LayoutParams.WRAP_CONTENT,
        ).apply {
            gravity = Gravity.TOP or Gravity.START
            leftMargin = left
            topMargin = top
        }
        content.controlDialScroll.layoutParams = FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.WRAP_CONTENT,
            ViewGroup.LayoutParams.WRAP_CONTENT,
        ).apply {
            gravity = Gravity.BOTTOM or Gravity.END
            rightMargin = content.root.width - (left + fab.width)
            bottomMargin = content.root.height - top + gap
        }
        content.controlDialScroll.visibility = View.VISIBLE
    }

    private fun capHeight() {
        val fabTop = (content.controlDialFab.layoutParams as FrameLayout.LayoutParams).topMargin
        val max = fabTop - gap
        val columnHeight = content.controlDialItems.height
        val target = when {
            columnHeight <= 0 -> ViewGroup.LayoutParams.WRAP_CONTENT
            columnHeight > max -> max.coerceAtLeast(gap)
            else -> columnHeight
        }
        val params = content.controlDialScroll.layoutParams
        if (params.height != target) {
            params.height = target
            content.controlDialScroll.layoutParams = params
        }
    }

    private fun rebuild(newItems: List<SpeedDialItem>) {
        rows.forEach { it.view.animate().cancel() }
        content.controlDialItems.removeAllViews()
        rows.clear()
        newItems.forEach { item ->
            val binding = ItemControlSpeedDialActionBinding.inflate(
                LayoutInflater.from(context),
                content.controlDialItems,
                false,
            )
            content.controlDialItems.addView(binding.root)
            rows += Row(item.key, binding)
        }
        bindRows()
        if (isOpen) rows.forEach(::resetRowState)
    }

    private fun resetRowState(row: Row) {
        row.view.translationY = 0f
        row.view.scaleX = 1f
        row.view.scaleY = 1f
        row.view.alpha = 1f
    }

    private fun bindRows() {
        rows.forEach { it.bind() }
    }

    private fun animateIn() {
        rows.forEachIndexed(::animateRowIn)
    }

    private fun animateRowIn(index: Int, row: Row) {
        val view = row.view
        view.animate().cancel()
        view.translationY = deltaToFab(view)
        view.scaleX = ITEM_SCALE
        view.scaleY = ITEM_SCALE
        view.alpha = 0f
        view.animate()
            .translationY(0f)
            .scaleX(1f)
            .scaleY(1f)
            .alpha(1f)
            .setStartDelay((rows.lastIndex - index) * stagger)
            .setDuration(itemDuration)
            .setInterpolator(AnimationUtils.FAST_OUT_SLOW_IN_INTERPOLATOR)
            .start()
    }

    private fun animateOut(index: Int, row: Row) {
        row.view.animate().cancel()
        row.view.animate()
            .translationY(deltaToFab(row.view))
            .scaleX(ITEM_SCALE)
            .scaleY(ITEM_SCALE)
            .alpha(0f)
            .setStartDelay(index * stagger)
            .setDuration(itemDuration)
            .setInterpolator(AnimationUtils.FAST_OUT_SLOW_IN_INTERPOLATOR)
            .start()
    }

    private fun fadeOut() {
        if (isOpen) return
        content.controlDialScrim.animate()
            .alpha(0f)
            .setDuration(scrimDuration)
            .withEndAction {
                if (!isOpen) popup.dismiss()
            }
            .start()
    }

    /** How far a row must travel to sit exactly on the speed-dial FAB. */
    internal fun deltaToFab(view: View): Float {
        val fabLocation = IntArray(2).also(content.controlDialFab::getLocationOnScreen)
        val rowLocation = IntArray(2).also(view::getLocationOnScreen)
        val fabCenter = fabLocation[1] + content.controlDialFab.height / 2f
        val rowCenter = rowLocation[1] + view.height / 2f
        return fabCenter - rowCenter
    }

    private inner class Row(val key: String, val binding: ItemControlSpeedDialActionBinding) {
        val view: View get() = binding.root

        fun bind() {
            val item = items.first { it.key == key }
            val label = item.label(context)
            val enabled = item.isEnabled()
            val active = item.isActive()
            binding.controlDialLabel.text = label
            binding.controlDialAction.setImageResource(item.icon())
            binding.controlDialAction.backgroundTintList = ColorStateList.valueOf(
                MaterialColors.getColor(
                    binding.controlDialAction,
                    if (active) com.google.android.material.R.attr.colorPrimaryContainer
                    else com.google.android.material.R.attr.colorSurfaceContainerHighest,
                ),
            )
            binding.controlDialAction.imageTintList = ColorStateList.valueOf(
                MaterialColors.getColor(
                    binding.controlDialAction,
                    if (active) com.google.android.material.R.attr.colorOnPrimaryContainer
                    else com.google.android.material.R.attr.colorOnSurfaceVariant,
                ),
            )
            binding.controlDialAction.alpha = if (enabled) 1f else DISABLED_ALPHA
            binding.controlDialAction.isEnabled = enabled
            binding.controlDialAction.contentDescription = item.description(context)
            binding.controlDialAction.setOnClickListener {
                if (!enabled) return@setOnClickListener
                close()
                onClick(item)
            }
            binding.controlDialAction.setOnLongClickListener {
                onLongClick(item)
                true
            }
        }
    }

    private companion object {
        const val ITEM_SCALE = 0.6f
        const val DISABLED_ALPHA = 0.5f
    }
}

internal fun SpeedDialItem.label(context: Context): String = when (this) {
    is SpeedDialItem.Tile -> tile.label(context)
    is SpeedDialItem.Wake -> context.getString(
        when (action.kind) {
            WakeFeedbackKind.FALSE_ALARM -> R.string.control_wake_false_alarm
            WakeFeedbackKind.MISSED -> R.string.control_wake_missed
        },
    )
}

private fun QuickTile.label(context: Context): String = when (this) {
    is QuickTile.Device -> entry.localizedDisplayName(context)
    is QuickTile.ScenarioTile -> scenario.label
    is QuickTile.Speak -> entry.localizedDisplayName(context)
    QuickTile.Call -> context.getString(R.string.control_call_title)
}

internal fun SpeedDialItem.description(context: Context): String = when (this) {
    is SpeedDialItem.Tile -> (tile as? QuickTile.Device)
        ?.entry
        ?.localizedStateText(context, R.string.control_state_unavailable)
        ?.let { state -> context.getString(R.string.control_tile_description, label(context), state) }
        ?: label(context)
    is SpeedDialItem.Wake -> label(context)
}

internal fun SpeedDialItem.icon(): Int = when (this) {
    is SpeedDialItem.Tile -> tile.icon()
    is SpeedDialItem.Wake -> when (action.kind) {
        WakeFeedbackKind.FALSE_ALARM -> R.drawable.control_ic_ear_off
        WakeFeedbackKind.MISSED -> R.drawable.control_ic_ear
    }
}

private fun QuickTile.icon(): Int = when (this) {
    is QuickTile.Device -> ControlIcons.resolve(entry)
    is QuickTile.ScenarioTile -> R.drawable.control_ic_play_circle
    is QuickTile.Speak -> R.drawable.control_ic_voice
    QuickTile.Call -> R.drawable.control_ic_call
}

internal fun SpeedDialItem.isActive(): Boolean = when (this) {
    is SpeedDialItem.Tile -> (tile as? QuickTile.Device)?.entry?.power == PowerState.ON ||
        tile !is QuickTile.Device
    is SpeedDialItem.Wake -> true
}

internal fun SpeedDialItem.isEnabled(): Boolean = when (this) {
    is SpeedDialItem.Tile -> when (val current = tile) {
        is QuickTile.Device -> current.entry.available && !current.busy
        is QuickTile.ScenarioTile -> !current.busy
        is QuickTile.Speak -> current.entry.available && !current.busy
        QuickTile.Call -> true
    }
    is SpeedDialItem.Wake -> action.entry.available && !action.busy
}
