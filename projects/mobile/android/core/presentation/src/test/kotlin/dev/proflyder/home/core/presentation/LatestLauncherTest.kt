package dev.proflyder.home.core.presentation

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class LatestLauncherTest {
    @Test
    fun aNewRequestCancelsTheStaleOne() = runTest(UnconfinedTestDispatcher()) {
        val launcher by backgroundScope.latestLauncher()
        val results = mutableListOf<String>()
        val first = CompletableDeferred<String>()
        val second = CompletableDeferred<String>()
        val stale = launcher.launch { results += first.await() }
        launcher.launch { results += second.await() }
        first.complete("stale")
        second.complete("fresh")
        assertFalse(stale.isActive)
        assertEquals(listOf("fresh"), results)
        assertFalse(launcher.isActive)
    }
}
