package dev.proflyder.home

import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class MainActivitySmokeTest {
    @Test
    fun coldLaunchCreatesActivityAndRootFragmentWithoutCrashing() {
        val controller = Robolectric.buildActivity(MainActivity::class.java).setup()
        val activity = controller.get()
        shadowOf(activity.mainLooper).idle()
        activity.supportFragmentManager.executePendingTransactions()

        assertFalse(activity.isFinishing)
        assertNotNull(activity.findViewById<android.view.View>(R.id.fragment_container))
        assertNotNull(activity.supportFragmentManager.findFragmentById(R.id.fragment_container))
    }
}
