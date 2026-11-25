package dev.proflyder.home.core.ui

import android.app.Dialog
import android.os.Looper
import androidx.fragment.app.FragmentActivity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class StateDialogSlotTest {
    private val activity = Robolectric.buildActivity(FragmentActivity::class.java).setup().get()
    private val slot = StateDialogSlot()
    private val created = mutableListOf<Dialog>()
    private var userCloses = 0

    private fun show(key: Any) = slot.show(key, { userCloses++ }) { Dialog(activity).also(created::add) }

    private fun idle() = shadowOf(Looper.getMainLooper()).idle()

    @Test
    fun theSameKeyKeepsTheShownDialog() {
        show(KEY)
        show(KEY)
        assertEquals(1, created.size)
        assertTrue(slot.isShowing(KEY))
    }

    @Test
    fun aUserCloseIsReportedAndTheSameKeyOpensAgain() {
        show(KEY)
        created.single().cancel()
        idle()
        assertEquals(1, userCloses)
        assertFalse(slot.isShowing(KEY))

        show(KEY)
        assertEquals(2, created.size)
        assertNotSame(created[0], created[1])
        assertTrue(created[1].isShowing)
    }

    @Test
    fun aProgrammaticClearIsNotReportedAsAUserClose() {
        show(KEY)
        val dialog = created.single()
        slot.clear()
        idle()
        assertFalse(dialog.isShowing)
        assertEquals(0, userCloses)
    }

    @Test
    fun anotherKeyReplacesTheDialogSilently() {
        show(KEY)
        show(OTHER)
        idle()
        assertFalse(created[0].isShowing)
        assertSame(created[1], created.last())
        assertTrue(slot.isShowing(OTHER))
        assertEquals(0, userCloses)
    }

    private companion object {
        const val KEY = "first"
        const val OTHER = "second"
    }
}
