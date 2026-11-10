package dev.proflyder.home.feature.control.presentation

import android.content.Context
import android.content.res.ColorStateList
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.LinearLayout
import androidx.core.widget.NestedScrollView
import com.google.android.material.animation.AnimationUtils
import com.google.android.material.color.MaterialColors
import com.google.android.material.floatingactionbutton.FloatingActionButton
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.feature.control.QuickTile
import dev.proflyder.home.feature.control.SpeedDialItem
import dev.proflyder.home.feature.control.WakeFeedbackKind
import dev.proflyder.home.feature.control.impl.R
import dev.proflyder.home.feature.control.impl.databinding.ItemControlSpeedDialActionBinding

/**
 * The Home quick-actions speed dial: a scrim, a vertical column of the same
 * FAB component as the root action, and plain labels to their left. Items fly
 * out from under the root FAB in a bottom-up cascade; closing runs the reverse
 * animation and then fades the scrim. The column scrolls when it does not fit.
 */
internal class SpeedDialOverlay(
    private val root: ViewGroup,
    private val overlay: View,
    private val scrim: View,
    private val scroll: NestedScrollView,
    private val column: LinearLayout,
    private val fab: FloatingActionButton,
    private val onClick: (SpeedDialItem) -> Unit,
    private val onLongClick: (SpeedDialItem) -> Unit,
    private val onOpenChanged: (Boolean) -> Unit,
) {
    private val rows = mutableListOf<Row>()
    private var items: List<SpeedDialItem> = emptyList()
    private val scrimDuration = root.resources.getInteger(R.integer.control_speed_dial_scrim_ms).toLong()
    private val itemDuration = root.resources.getInteger(R.integer.control_speed_dial_item_ms).toLong()
    private val stagger = root.resources.getInteger(R.integer.control_speed_dial_stagger_ms).toLong()

    var isOpen = false
        private set

    init {
        scrim.setOnClickListener { close() }
        fab.setOnClickListener { toggle() }
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

    fun toggle() {
        when (isOpen) {
            true -> close()
            false -> open()
        }
    }

    fun open() {
        if (isOpen) return
        isOpen = true
        overlay.visibility = View.VISIBLE
        scrim.animate().cancel()
        scrim.alpha = 0f
        scrim.animate().alpha(1f).setDuration(scrimDuration).start()
        fab.animate().cancel()
        fab.show()
        fab.setImageResource(R.drawable.control_ic_close)
        fab.contentDescription = root.context.getString(R.string.control_quick_fab_close)
        fab.rotation = -90f
        fab.animate()
            .rotation(0f)
            .setDuration(itemDuration)
            .setInterpolator(AnimationUtils.FAST_OUT_SLOW_IN_INTERPOLATOR)
            .start()
        onOpenChanged(true)
        overlay.post {
            capHeight()
            scroll.post { animateIn() }
        }
    }

    fun close() {
        if (!isOpen) return
        isOpen = false
        onOpenChanged(false)
        fab.animate()
            .rotation(-90f)
            .setDuration(itemDuration)
            .setInterpolator(AnimationUtils.FAST_OUT_SLOW_IN_INTERPOLATOR)
            .start()
        rows.forEachIndexed { index, row ->
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
        val total = rows.size.coerceAtLeast(1) * stagger + itemDuration
        scrim.postDelayed({ fadeOut() }, total)
    }

    /** Dismiss without animation when the view is going away. */
    fun reset() {
        scrim.animate().cancel()
        rows.forEach { it.view.animate().cancel() }
        overlay.visibility = View.GONE
        scrim.alpha = 0f
        isOpen = false
        resetFab()
    }

    private fun rebuild(newItems: List<SpeedDialItem>) {
        rows.forEach { it.view.animate().cancel() }
        column.removeAllViews()
        rows.clear()
        newItems.forEach { item ->
            val binding = ItemControlSpeedDialActionBinding.inflate(
                LayoutInflater.from(root.context),
                column,
                false,
            )
            column.addView(binding.root)
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

    private fun capHeight() {
        val margin = root.resources.getDimensionPixelSize(R.dimen.control_speed_dial_fab_margin)
        val max = root.height - fab.height - margin * 2
        val content = column.height
        val target = when {
            content <= 0 -> ViewGroup.LayoutParams.WRAP_CONTENT
            content > max -> max.coerceAtLeast(margin)
            else -> content
        }
        val params = scroll.layoutParams
        if (params.height != target) {
            params.height = target
            scroll.layoutParams = params
        }
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

    private fun fadeOut() {
        if (isOpen) return
        scrim.animate()
            .alpha(0f)
            .setDuration(scrimDuration)
            .withEndAction {
                if (!isOpen) {
                    overlay.visibility = View.GONE
                    resetFab()
                }
            }
            .start()
    }

    private fun resetFab() {
        fab.setImageResource(R.drawable.control_ic_bolt)
        fab.rotation = 0f
        fab.contentDescription = root.context.getString(R.string.control_quick_fab)
    }

    /** How far a row must travel to sit exactly on the root FAB. */
    internal fun deltaToFab(view: View): Float {
        val fabLocation = IntArray(2).also(fab::getLocationOnScreen)
        val rowLocation = IntArray(2).also(view::getLocationOnScreen)
        val fabCenter = fabLocation[1] + fab.height / 2f
        val rowCenter = rowLocation[1] + view.height / 2f
        return fabCenter - rowCenter
    }

    private inner class Row(val key: String, val binding: ItemControlSpeedDialActionBinding) {
        val view: View get() = binding.root

        fun bind() {
            val item = items.first { it.key == key }
            val context = binding.root.context
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
