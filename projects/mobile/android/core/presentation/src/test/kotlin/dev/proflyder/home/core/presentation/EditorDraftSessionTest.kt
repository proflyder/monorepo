package dev.proflyder.home.core.presentation

import androidx.lifecycle.SavedStateHandle
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Test

class EditorDraftSessionTest {
    private enum class Done { SAVED }

    private object TextCodec : EditorDraftCodec<String> {
        override fun encode(draft: String): Map<String, Any?> = mapOf("note_text" to draft)

        override fun decode(values: Map<String, Any?>): String? = values["note_text"] as? String
    }

    private fun session(state: SavedStateHandle, endpoint: String = "https://a") =
        EditorDraftSession<String, String, Done>(state, "note", TextCodec, Done.entries) { endpoint }

    @Test
    fun anAmbiguousFailureKeepsTheCreateKeyAcrossProcessDeath() = runTest {
        val state = SavedStateHandle()
        val keys = mutableListOf<String>()
        val first = session(state)
        first.persist("milk")
        first.submit("milk") { key -> Outcome.Failure(RemoteFailure.Timeout).also { keys += key } }
        val restored = session(SavedStateHandle(state.keys().associateWith { state.get<Any?>(it) }))
        assertEquals("milk", restored.restored)
        restored.adoptRestoredCreate("milk")
        restored.submit("milk") { key -> Outcome.Success(Unit).also { keys += key } }
        assertEquals(keys[0], keys[1])
    }

    @Test
    fun anEditOrAnotherServerStartsANewKey() = runTest {
        val state = SavedStateHandle()
        val keys = mutableListOf<String>()
        val first = session(state)
        first.submit("milk") { key -> Outcome.Failure(RemoteFailure.Timeout).also { keys += key } }
        first.edited("bread")
        first.submit("bread") { key -> Outcome.Failure(RemoteFailure.Timeout).also { keys += key } }
        assertNotEquals(keys[0], keys[1])
        val elsewhere = session(state, endpoint = "https://b")
        elsewhere.adoptRestoredCreate("bread")
        elsewhere.submit("bread") { key -> Outcome.Success(Unit).also { keys += key } }
        assertNotEquals(keys[1], keys[2])
    }

    @Test
    fun completionSurvivesUntilConsumed() {
        val state = SavedStateHandle()
        session(state).complete(Done.SAVED)
        assertEquals(Done.SAVED, session(state).pendingCompletion)
        session(state).consumeCompletion()
        assertNull(session(state).pendingCompletion)
    }
}
