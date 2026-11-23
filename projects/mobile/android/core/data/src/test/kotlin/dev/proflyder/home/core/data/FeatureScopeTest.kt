package dev.proflyder.home.core.data

import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class FeatureScopeTest {
    private class Graph(val lifetime: FeatureLifetime)

    private val background = MutableStateFlow(false)

    private fun TestScope.scope() = RefCountedScope(
        host = TestScope(UnconfinedTestDispatcher(testScheduler)).backgroundScope,
        background = background,
        releaseDelay = 5.seconds,
        create = ::Graph,
    )

    @Test
    fun allOwnersShareOneInstance() = runTest {
        val scope = scope()
        val first = scope.acquire()
        val second = scope.acquire()
        assertSame(first.graph, second.graph)
        assertEquals(2, scope.owners)
    }

    @Test
    fun theInstanceIsClearedShortlyAfterTheLastRelease() = runTest {
        val scope = scope()
        val lease = scope.acquire()
        val lifetime = lease.graph.lifetime
        lease.close()
        lease.close()
        assertEquals(0, scope.owners)
        assertNotNull(scope.instance)
        advanceTimeBy(6.seconds)
        runCurrent()
        assertNull(scope.instance)
        assertFalse(lifetime.scope.isActive)
    }

    @Test
    fun aQuickReturnReusesTheInstance() = runTest {
        val scope = scope()
        val first = scope.acquire().also { it.close() }.graph
        advanceTimeBy(2.seconds)
        val again = scope.acquire()
        advanceTimeBy(10.seconds)
        assertSame(first, again.graph)
        assertNotNull(scope.instance)
    }

    @Test
    fun goingToBackgroundClearsAnIdleScopeAtOnce() = runTest {
        val scope = scope()
        val first = scope.acquire().also { it.close() }.graph
        background.value = true
        runCurrent()
        assertNull(scope.instance)
        background.value = false
        assertNotSame(first, scope.acquire().graph)
    }

    @Test
    fun goingToBackgroundKeepsAScopeWithOwners() = runTest {
        val scope = scope()
        scope.acquire()
        background.value = true
        runCurrent()
        assertNotNull(scope.instance)
    }
}
