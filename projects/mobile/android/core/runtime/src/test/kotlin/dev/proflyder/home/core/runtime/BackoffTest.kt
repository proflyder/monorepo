package dev.proflyder.home.core.runtime

import java.io.IOException
import kotlin.time.Duration.Companion.seconds
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class BackoffTest {
    private val backoff = Backoff(initial = 1.seconds, max = 30.seconds)

    @Test
    fun delaysDoubleUpToTheCap() {
        val delays = (0L..6L).map { backoff.delayFor(it).inWholeSeconds }
        assertEquals(listOf(1L, 2L, 4L, 8L, 16L, 30L, 30L), delays)
    }

    @Test
    fun hugeAttemptsStayAtTheCap() {
        assertEquals(30.seconds, backoff.delayFor(Long.MAX_VALUE))
    }

    @Test
    fun retryWaitsTheBackoffBetweenAttempts() = runTest {
        var attempts = 0
        val value = flow {
            attempts += 1
            if (attempts < 3) throw IOException("down")
            emit(attempts)
        }.retryWithBackoff(backoff).first()
        assertEquals(3, value)
        assertEquals(3_000L, currentTime)
    }

    @Test(expected = IllegalStateException::class)
    fun failuresThatShouldNotRetryAreRethrown() = runTest {
        flow<Int> { throw IllegalStateException("fatal") }
            .retryWithBackoff(backoff) { it is IOException }
            .first()
    }

    @Test
    fun reconnectingResubscribesAfterFailuresAndCompletions() = runTest {
        var connections = 0
        val values = flow {
            connections += 1
            if (connections == 2) throw IOException("dropped")
            emit(connections)
        }.reconnecting(backoff) { it is IOException }.take(2).toList()
        assertEquals(listOf(1, 3), values)
        assertEquals(2_000L, currentTime)
    }

    @Test(expected = IllegalStateException::class)
    fun reconnectingRethrowsFailuresThatShouldNotRetry() = runTest {
        flow<Int> { throw IllegalStateException("fatal") }
            .reconnecting(backoff) { it is IOException }
            .first()
    }
}
