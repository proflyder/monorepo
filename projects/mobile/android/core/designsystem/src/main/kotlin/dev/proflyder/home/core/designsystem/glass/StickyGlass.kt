package dev.proflyder.home.core.designsystem.glass

import android.view.View
import androidx.recyclerview.widget.RecyclerView
import dev.proflyder.home.core.designsystem.list.StickyHeaderListener
import dev.proflyder.home.core.designsystem.list.StickyHeadersLayoutManager

class StickyGlass private constructor(
    private val host: GlassBlockHost,
    private val manager: StickyHeadersLayoutManager,
) : StickyHeaderListener, View.OnAttachStateChangeListener {
    private var block: View? = null

    override fun onStickyHeaderChanged(view: View?) {
        block?.let(host::detachBlock)
        block = view?.also(host::attachBlock)
    }

    override fun onStickyHeaderMoved(view: View, edge: Int) = host.blockMoved()

    override fun onViewAttachedToWindow(view: View) = onStickyHeaderChanged(manager.stickyHeaderView)

    override fun onViewDetachedFromWindow(view: View) {
        block?.let(host::detachBlock)
        block = null
    }

    companion object {
        fun attach(list: RecyclerView) {
            val manager = list.layoutManager as? StickyHeadersLayoutManager ?: return
            val host = list.context.glassBlockHost() ?: return
            val glass = StickyGlass(host, manager)
            manager.addStickyHeaderListener(glass)
            list.addOnAttachStateChangeListener(glass)
        }
    }
}
