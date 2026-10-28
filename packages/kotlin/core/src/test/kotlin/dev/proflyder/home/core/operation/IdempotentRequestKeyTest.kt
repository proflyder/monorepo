package dev.proflyder.home.core.operation

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class IdempotentRequestKeyTest {
    @Test
    fun ambiguousRetryWithSamePayloadReusesKey() {
        var counter = 0
        val identity = IdempotentRequestKey<String> { "key-${++counter}" }
        val payload = "alarm:label=Wake;hour=7"

        val firstAttempt = identity.keyFor(payload)
        // The server may have accepted this request while the response was lost.
        // No markSucceeded() is called, so the retry must use the same key.
        val retry = identity.keyFor(payload)

        assertEquals("key-1", firstAttempt)
        assertEquals("key-1", retry)
        assertEquals("key-1", identity.pendingOrNull())
    }

    @Test
    fun newIntendedCreationGetsNewKeyAfterSuccess() {
        var counter = 0
        val identity = IdempotentRequestKey<String> { "key-${++counter}" }
        val payload = "timer:Tea:300"

        assertEquals("key-1", identity.keyFor(payload))
        identity.markSucceeded()
        assertNull(identity.pendingOrNull())
        assertEquals("key-2", identity.keyFor(payload))
    }

    @Test
    fun changedPayloadStartsNewOperation() {
        var counter = 0
        val identity = IdempotentRequestKey<String> { "key-${++counter}" }

        assertEquals("key-1", identity.keyFor("duration=60"))
        assertEquals("key-2", identity.keyFor("duration=90"))
        assertEquals("key-2", identity.keyFor("duration=90"))
    }

    @Test
    fun adoptedPendingKeyIsReusedAfterProcessRecreation() {
        var counter = 0
        val identity = IdempotentRequestKey<String> { "key-${++counter}" }
        identity.adopt("draft", "restored-key")
        assertEquals("restored-key", identity.keyFor("draft"))
        assertEquals("key-1", identity.keyFor("different"))
    }

    @Test
    fun resetAfterDefiniteRejectionStartsNewOperation() {
        var counter = 0
        val identity = IdempotentRequestKey<String> { "key-${++counter}" }

        assertEquals("key-1", identity.keyFor("draft"))
        identity.reset()
        assertEquals("key-2", identity.keyFor("draft"))
    }

    @Test
    fun defaultGeneratorProducesDistinctKeys() {
        val identity = IdempotentRequestKey<String>()
        val first = identity.keyFor("draft")
        identity.markSucceeded()
        val second = identity.keyFor("draft")
        assertEquals(36, first.length)
        assertEquals(36, second.length)
        org.junit.Assert.assertNotEquals(first, second)
    }
}
