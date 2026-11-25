package dev.proflyder.home.core.designsystem.list

import android.view.View
import android.view.ViewGroup
import androidx.recyclerview.widget.RecyclerView

class ListHeaderSpacer private constructor(
    private val list: RecyclerView,
    private val header: View,
) : RecyclerView.Adapter<RecyclerView.ViewHolder>() {
    private var spacer: View? = null
    private val follower = Follower()

    init {
        header.addOnLayoutChangeListener(follower)
        list.addOnLayoutChangeListener(follower)
        list.addOnScrollListener(follower)
    }

    override fun getItemCount(): Int = 1

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): RecyclerView.ViewHolder {
        val view = View(parent.context).apply { layoutParams = RecyclerView.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, header.height) }
        spacer = view
        return object : RecyclerView.ViewHolder(view) {}
    }

    override fun onBindViewHolder(holder: RecyclerView.ViewHolder, position: Int) {
        holder.itemView.layoutParams.height = header.height
    }

    private fun follow() {
        val view = spacer?.takeIf { it.parent === list }
        header.translationY = when (view) {
            null -> -header.height.toFloat()
            else -> (list.top + view.top - header.top).toFloat()
        }
    }

    private fun resize() {
        val view = spacer ?: return
        if (view.layoutParams.height == header.height) return
        view.layoutParams.height = header.height
        list.post(view::requestLayout)
    }

    private inner class Follower : RecyclerView.OnScrollListener(), View.OnLayoutChangeListener {
        override fun onScrolled(recyclerView: RecyclerView, dx: Int, dy: Int) = follow()

        override fun onLayoutChange(view: View, l: Int, t: Int, r: Int, b: Int, ol: Int, ot: Int, or: Int, ob: Int) {
            if (view === header) resize()
            follow()
        }
    }

    companion object {
        fun leading(list: RecyclerView, header: View): ListHeaderSpacer = ListHeaderSpacer(list, header)
    }
}
