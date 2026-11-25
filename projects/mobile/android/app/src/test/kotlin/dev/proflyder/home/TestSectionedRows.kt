package dev.proflyder.home

import android.content.Context
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.TextView
import androidx.annotation.IdRes
import androidx.annotation.LayoutRes
import androidx.recyclerview.widget.RecyclerView
import com.google.android.material.card.MaterialCardView
import dev.proflyder.home.core.designsystem.list.SectionedAdapter

internal class TestSectionedRows(
    context: Context,
    @LayoutRes private val headerLayout: Int,
    private val rows: List<String> = emptyList(),
    private val bindHeader: (View, Int) -> Unit,
) : RecyclerView.Adapter<RecyclerView.ViewHolder>(), SectionedAdapter {
    private val inflater = LayoutInflater.from(context)
    private val density = context.resources.displayMetrics.density

    override fun isSectionHeader(position: Int): Boolean = position % SECTION_SIZE == 0

    override fun getItemCount(): Int = SECTIONS * (ROWS + 1)

    override fun getItemViewType(position: Int): Int = if (isSectionHeader(position)) HEADER else ROW

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder = when (viewType) {
        HEADER -> object : RecyclerView.ViewHolder(inflater.inflate(headerLayout, parent, false)) {}
        else -> object : RecyclerView.ViewHolder(card(parent)) {}
    }

    override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
        when {
            isSectionHeader(position) -> bindHeader(holder.itemView, position / SECTION_SIZE)
            else -> ((holder.itemView as MaterialCardView).getChildAt(0) as TextView).text = rowTitle(position)
        }
    }

    private fun rowTitle(position: Int): String = rows.getOrNull(position % rows.size.coerceAtLeast(1)) ?: "Row $position"

    private fun card(parent: ViewGroup): MaterialCardView {
        val margin = (MARGIN_DP * density).toInt()
        val card = MaterialCardView(parent.context).apply {
            layoutParams = RecyclerView.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, (ROW_DP * density).toInt()).apply {
                topMargin = margin
                bottomMargin = margin
            }
            radius = RADIUS_DP * density
        }
        card.addView(
            TextView(parent.context).apply {
                setTextAppearance(com.google.android.material.R.style.TextAppearance_Material3_BodyLarge)
                setPadding(margin * 3, margin * 3, margin * 3, margin * 3)
            },
            FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT),
        )
        return card
    }

    companion object {
        fun titled(context: Context, @LayoutRes layout: Int, @IdRes title: Int, titles: List<String>, rows: List<String> = emptyList()) =
            TestSectionedRows(context, layout, rows) { view, section -> view.findViewById<TextView>(title).text = titles[section % titles.size] }

        private const val SECTIONS = 4
        private const val ROWS = 6
        const val SECTION_SIZE = ROWS + 1
        private const val HEADER = 0
        private const val ROW = 1
        private const val MARGIN_DP = 6
        private const val ROW_DP = 72
        private const val RADIUS_DP = 16f
    }
}
