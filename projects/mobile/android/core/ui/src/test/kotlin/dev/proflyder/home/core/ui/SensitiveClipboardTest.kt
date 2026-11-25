package dev.proflyder.home.core.ui

import android.content.ClipboardManager
import android.content.Context
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class SensitiveClipboardTest {
    private fun clipboard(): ClipboardManager =
        RuntimeEnvironment.getApplication().getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager

    @Test
    fun aCopiedValueClearsAfterThirtySeconds() = runTest {
        val clipboard = clipboard()
        val copier = SensitiveClipboard(RuntimeEnvironment.getApplication(), LABEL, TestScope(testScheduler))

        copier.copy("s3cret")
        assertEquals("s3cret", clipboard.primaryClip?.getItemAt(0)?.text?.toString())

        advanceTimeBy(30_001)
        runCurrent()

        assertNull(clipboard.primaryClip)
    }

    @Test
    fun aCopyOrderedTwiceKeepsOnlyTheLatestForTheSameWindow() = runTest {
        val clipboard = clipboard()
        val copier = SensitiveClipboard(RuntimeEnvironment.getApplication(), LABEL, TestScope(testScheduler))

        copier.copy("first")
        advanceTimeBy(20_000)
        copier.copy("second")
        advanceTimeBy(20_000)
        runCurrent()

        assertEquals("second", clipboard.primaryClip?.getItemAt(0)?.text?.toString())
    }

    private companion object {
        const val LABEL = "label"
    }
}
