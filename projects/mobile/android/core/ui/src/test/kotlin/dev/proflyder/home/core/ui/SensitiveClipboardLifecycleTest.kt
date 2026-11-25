package dev.proflyder.home.core.ui

import android.content.ClipboardManager
import android.content.Context
import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import androidx.fragment.app.Fragment
import androidx.fragment.app.FragmentActivity
import java.time.Duration
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class SensitiveClipboardLifecycleTest {
    class CopyingFragment : Fragment() {
        private val clipboard by lazy { SensitiveClipboard.forProcess(requireContext(), LABEL) }

        override fun onCreateView(inflater: LayoutInflater, container: ViewGroup?, savedInstanceState: Bundle?): View =
            FrameLayout(inflater.context)

        override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
            clipboard.copy(SECRET)
        }
    }

    @Test
    fun theClearRunsAfterTheFragmentViewIsDestroyed() {
        val activity = Robolectric.buildActivity(FragmentActivity::class.java).setup().get()
        val fragment = CopyingFragment()
        activity.supportFragmentManager.beginTransaction().add(android.R.id.content, fragment).commitNow()
        assertEquals(SECRET, clipboard().primaryClip?.getItemAt(0)?.text?.toString())

        activity.supportFragmentManager.beginTransaction().remove(fragment).commitNow()
        shadowOf(android.os.Looper.getMainLooper()).idleFor(Duration.ofSeconds(31))

        assertNull(clipboard().primaryClip)
    }

    private fun clipboard(): ClipboardManager =
        RuntimeEnvironment.getApplication().getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager

    private companion object {
        const val LABEL = "label"
        const val SECRET = "s3cret"
    }
}
