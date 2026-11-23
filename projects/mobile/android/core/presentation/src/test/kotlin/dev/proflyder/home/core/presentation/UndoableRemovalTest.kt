package dev.proflyder.home.core.presentation

import dev.proflyder.home.core.data.Page
import dev.proflyder.home.core.data.PagedList
import dev.proflyder.home.core.data.PagedSource
import dev.proflyder.home.core.data.PageRequest
import dev.proflyder.home.core.data.PagingAction
import dev.proflyder.home.core.data.RefreshMode
import dev.proflyder.home.core.model.Keyed
import dev.proflyder.home.core.model.Outcome
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class UndoableRemovalTest {
    @Test
    fun removingHidesTheItemAndKeepsItForUndo() = runTest {
        val list = loadedList()
        val removal = removal(list)

        removal.remove(Row("a"))
        runCurrent()

        assertEquals(listOf("b"), list.state.value.items.map { it.key })
        assertEquals("a", removal.pending.value?.key)
    }

    @Test
    fun undoRevealsTheItemWithoutCommitting() = runTest {
        val list = loadedList()
        val commits = mutableListOf<String>()
        val removal = removal(list, commits)

        removal.remove(Row("a"))
        removal.undo()
        advanceUntilIdle()

        assertEquals(listOf("a", "b"), list.state.value.items.map { it.key })
        assertNull(removal.pending.value)
        assertTrue(commits.isEmpty())
    }

    @Test
    fun theWindowExpiryCommitsTheRemoval() = runTest {
        val list = loadedList()
        val commits = mutableListOf<String>()
        val removal = removal(list, commits)

        removal.remove(Row("a"))
        advanceTimeBy(101)
        runCurrent()

        assertEquals(listOf("a"), commits)
        assertNull(removal.pending.value)
    }

    @Test
    fun closingCommitsAPendingRemoval() = runTest {
        val list = loadedList()
        val commits = mutableListOf<String>()
        val removal = removal(list, commits)

        removal.remove(Row("a"))
        removal.close()
        advanceUntilIdle()

        assertEquals(listOf("a"), commits)
    }

    @Test
    fun aSecondRemovalCommitsTheFirstOne() = runTest {
        val list = loadedList()
        val commits = mutableListOf<String>()
        val removal = removal(list, commits)

        removal.remove(Row("a"))
        removal.remove(Row("b"))
        runCurrent()

        assertEquals(listOf("a"), commits)
        assertEquals("b", removal.pending.value?.key)
    }

    private fun TestScope.loadedList(): PagedList<Row> {
        val list = PagedList(this, object : PagedSource<Row> {
            override suspend fun load(request: PageRequest) = Outcome.Success(Page(listOf(Row("a"), Row("b")), null))
        })
        list.dispatch(PagingAction.Refresh(RefreshMode.HARD))
        advanceUntilIdle()
        return list
    }

    private fun TestScope.removal(list: PagedList<Row>, commits: MutableList<String> = mutableListOf()) =
        UndoableRemoval(
            list = list,
            timerScope = this,
            commitScope = this,
            commit = { commits += it.key },
            duration = kotlin.time.Duration.parse("100ms"),
        )

    private data class Row(override val key: String) : Keyed
}
