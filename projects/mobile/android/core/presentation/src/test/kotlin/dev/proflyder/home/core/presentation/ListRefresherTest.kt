package dev.proflyder.home.core.presentation

import dev.proflyder.home.core.data.ListLoadState
import dev.proflyder.home.core.data.LoadStep
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.data.valueOr
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ListRefresherTest {
    @Test
    fun onlyTheLatestLoadReachesTheState() = runTest(UnconfinedTestDispatcher()) {
        val replies = ArrayDeque(listOf(CompletableDeferred<Outcome<Int>>(), CompletableDeferred()))
        val pending = replies.toList()
        var state = ListLoadState()
        var value = 0
        val refresher = ListRefresher(backgroundScope, { replies.removeFirst().await() }) { step ->
            state = state.after(step)
            value = step.valueOr(value)
        }
        refresher.refresh(RefreshMode.SOFT)
        refresher.refresh(RefreshMode.SOFT)
        pending[0].complete(Outcome.Success(1))
        assertTrue(state.refreshing)
        pending[1].complete(Outcome.Success(2))
        assertEquals(2, value)
        assertEquals(ListLoadState(refreshing = false), state)
    }

    @Test
    fun softRefreshKeepsTheIndicatorAndFailureIsReported() {
        val idle = ListLoadState(refreshing = false)
        assertFalse(idle.after(LoadStep.Started(RefreshMode.SOFT)).refreshing)
        assertTrue(idle.after(LoadStep.Started(RefreshMode.HARD)).refreshing)
        val failed = idle.after(LoadStep.Failed(RemoteFailure.Timeout))
        assertEquals(RemoteFailure.Timeout, failed.failure)
        assertEquals(null, failed.after(LoadStep.Started(RefreshMode.SOFT)).failure)
    }
}
