package dev.proflyder.home.core.presentation

import android.os.Looper
import androidx.fragment.app.Fragment
import androidx.fragment.app.FragmentActivity
import dev.proflyder.home.core.presentation.ScreenLifecycleTest.OtherTitle
import dev.proflyder.home.core.presentation.ScreenLifecycleTest.PickedTitle
import dev.proflyder.home.core.presentation.ScreenLifecycleTest.ProbeScreen
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ScreenResultsTest {
    @Test
    fun aResultFromAChildReachesTheViewModelAfterTheViewWasRecreated() {
        val activity = Robolectric.buildActivity(FragmentActivity::class.java).setup().get()
        val screen = ProbeScreen()
        val manager = activity.supportFragmentManager
        manager.beginTransaction().add(android.R.id.content, screen, TAG).commitNow()
        manager.beginTransaction().detach(screen).commitNow()
        manager.beginTransaction().attach(screen).commitNow()
        val child = Fragment()
        screen.childFragmentManager.beginTransaction().add(child, "child").commitNow()

        child.setResult(PickedTitle, "picked")
        idle()

        assertEquals("picked", screen.probe.state.value.title)
        assertEquals(1, screen.resultListeners)
    }

    @Test
    fun aChildRestoredAfterRotationReachesTheRecreatedScreen() {
        val controller = Robolectric.buildActivity(FragmentActivity::class.java).setup()
        val before = ProbeScreen()
        controller.get().supportFragmentManager.beginTransaction().add(android.R.id.content, before, TAG).commitNow()
        before.childFragmentManager.beginTransaction().add(Fragment(), CHILD).commitNow()
        val saved = android.os.Bundle()
        controller.pause().stop().saveInstanceState(saved).destroy()

        val restored = Robolectric.buildActivity(FragmentActivity::class.java).setup(saved)
        val after = restored.get().supportFragmentManager.findFragmentByTag(TAG) as ProbeScreen
        val child = checkNotNull(after.childFragmentManager.findFragmentByTag(CHILD))
        child.setResult(PickedTitle, "after rotation")
        idle()

        assertEquals("after rotation", after.probe.state.value.title)
        assertEquals(1, after.resultListeners)
    }

    @Test
    fun aResultWaitsWhileTheScreenIsOnTheBackStack() {
        val activity = Robolectric.buildActivity(FragmentActivity::class.java).setup().get()
        val screen = ProbeScreen()
        val manager = activity.supportFragmentManager
        manager.beginTransaction().add(android.R.id.content, screen, TAG).commitNow()
        manager.beginTransaction().replace(android.R.id.content, Fragment()).addToBackStack(null).commit()
        manager.executePendingTransactions()

        screen.childFragmentManager.sendResult(PickedTitle, "from back stack")
        idle()
        assertEquals("start", screen.probe.state.value.title)

        manager.popBackStackImmediate()
        idle()
        assertEquals("from back stack", screen.probe.state.value.title)
    }

    @Test
    fun keysAreTypedAndDistinctEvenForTheSameValueType() {
        assertNotEquals(PickedTitle.requestKey, OtherTitle.requestKey)
        assertEquals("value", PickedTitle.unpack(PickedTitle.pack("value")))
        assertNull(PickedTitle.unpack(android.os.Bundle()))
    }

    private fun idle() = shadowOf(Looper.getMainLooper()).idle()

    private companion object {
        const val TAG = "probe"
        const val CHILD = "child"
    }
}
