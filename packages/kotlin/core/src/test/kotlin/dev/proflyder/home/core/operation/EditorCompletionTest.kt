package dev.proflyder.home.core.operation

import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class EditorCompletionTest {
    @Test
    fun completionIsRetainedUntilConsumedWithoutAnyCollector() {
        val completion = EditorCompletion<String>()
        // No collector is active (the Fragment is STOPPED).
        completion.publish("SAVED")
        assertEquals("SAVED", completion.state.value)
        assertEquals("SAVED", completion.consume())
        assertNull(completion.state.value)
        assertNull(completion.consume())
    }

    @Test
    fun resumedCollectorCanObserveAPreviouslyPublishedCompletion() = runBlocking {
        val completion = EditorCompletion<String>()
        completion.publish("DELETED")
        // Collection starts only after the Fragment resumes.
        assertEquals("DELETED", completion.state.first { it != null })
    }
}
