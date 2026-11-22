package dev.proflyder.home.core.presentation

import android.view.LayoutInflater
import android.view.ViewGroup
import android.widget.TextView
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import androidx.recyclerview.widget.SimpleItemAnimator
import dev.proflyder.home.core.ui.BaseAdapterCallback
import dev.proflyder.home.core.ui.BaseListAdapter
import dev.proflyder.home.core.ui.BaseViewHolder
import dev.proflyder.home.core.ui.ViewBinding
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ListAdapterAnimationTest {
    private class TextBinding(override val root: TextView) : ViewBinding

    private class TextAdapter : BaseListAdapter<String, TextBinding, BaseAdapterCallback<String>>(
        BaseAdapterCallback(),
        KeyedDiff({ it }),
    ) {
        override fun onCreateHolder(inflater: LayoutInflater, parent: ViewGroup, viewType: Int) =
            object : BaseViewHolder<String, TextBinding, BaseAdapterCallback<String>>(TextBinding(TextView(parent.context)), callback) {
                override fun bind(item: String) {
                    binding.root.text = item
                }
            }
    }

    @Test
    fun attachingAListAdapterTurnsOffCrossFadeChangeAnimations() {
        val list = RecyclerView(RuntimeEnvironment.getApplication())
        list.layoutManager = LinearLayoutManager(list.context)
        list.adapter = TextAdapter()
        assertFalse((list.itemAnimator as SimpleItemAnimator).supportsChangeAnimations)
    }
}
