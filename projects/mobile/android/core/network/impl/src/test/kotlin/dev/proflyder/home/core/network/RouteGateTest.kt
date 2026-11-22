package dev.proflyder.home.core.network

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class RouteGateTest {
    private class Condition(initial: Boolean) : RouteCondition {
        override val ready = MutableStateFlow(initial)
    }

    @Test
    fun opensWhenEveryConditionIsReady() = runTest {
        val network = Condition(true)
        val vpn = Condition(false)
        val waiting = async { ConditionRouteGate(setOf(network, vpn)).await() }
        runCurrent()
        assertFalse(waiting.isCompleted)
        vpn.ready.value = true
        runCurrent()
        assertTrue(waiting.isCompleted)
        assertEquals(0L, currentTime)
    }

    @Test
    fun givesUpAfterTheSingleRouteTimeout() = runTest {
        ConditionRouteGate(setOf(Condition(false))).await()
        assertEquals(ConditionRouteGate.ROUTE_WAIT.inWholeMilliseconds, currentTime)
    }

    @Test
    fun noConditionsMeansOpen() = runTest {
        ConditionRouteGate(emptySet()).await()
        assertEquals(0L, currentTime)
    }
}
