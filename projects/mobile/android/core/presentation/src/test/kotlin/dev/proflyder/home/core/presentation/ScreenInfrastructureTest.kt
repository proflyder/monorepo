package dev.proflyder.home.core.presentation

import android.os.Looper
import androidx.activity.ComponentDialog
import androidx.fragment.app.DialogFragment
import androidx.fragment.app.Fragment
import androidx.fragment.app.FragmentActivity
import dev.proflyder.home.core.testing.FakeAnalyticsTracker
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ScreenInfrastructureTest {
    @Test
    fun resumedTrackedScreensAreReportedOnce() {
        val tracker = FakeAnalyticsTracker()
        val activity = Robolectric.buildActivity(FragmentActivity::class.java).setup().get()
        activity.supportFragmentManager.registerFragmentLifecycleCallbacks(ScreenResumeTracker(tracker), true)

        activity.supportFragmentManager.beginTransaction().add(android.R.id.content, AlarmsFragment()).commitNow()
        activity.supportFragmentManager.beginTransaction().add(android.R.id.content, Fragment()).commitNow()

        assertEquals(listOf("Alarms"), tracker.screens)
    }

    @Test
    fun fragmentsInsideADialogUseTheDialogBackDispatcher() {
        val activity = Robolectric.buildActivity(FragmentActivity::class.java).setup().get()
        val dialog = HostDialog()
        dialog.show(activity.supportFragmentManager, "host")
        activity.supportFragmentManager.executePendingTransactions()
        shadowOf(Looper.getMainLooper()).idle()
        val inner = Fragment()
        dialog.childFragmentManager.beginTransaction().add(inner, "inner").commitNow()

        assertSame((dialog.requireDialog() as ComponentDialog).onBackPressedDispatcher, inner.backDispatcher)
    }

    @Test
    fun screenFragmentsOutsideDialogsUseTheActivityDispatcher() {
        val activity = Robolectric.buildActivity(FragmentActivity::class.java).setup().get()
        val plain = Fragment()
        activity.supportFragmentManager.beginTransaction().add(plain, "plain").commitNow()

        assertSame(activity.onBackPressedDispatcher, plain.backDispatcher)
    }

    class AlarmsFragment : Fragment(), TrackedScreen {
        override val screenName: String get() = screenNameOf(this)
    }

    class HostDialog : DialogFragment()
}
