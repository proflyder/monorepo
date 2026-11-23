package dev.proflyder.home.core.data

import dev.proflyder.home.core.model.Keyed
import dev.proflyder.home.core.model.RemoteFailure
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PagingReducerTest {
    private val reducer = PagingReducer(pageSize = 20)

    @Test
    fun aRefreshStartsAFullPageLoad() {
        val transition = reducer.start(state(), PagingAction.Refresh(RefreshMode.HARD), null)

        assertEquals(LoadState.Loading, transition.state.refresh)
        assertEquals(PagingEffect.Load(PageRequest(null, 20), LoadMode.Reset), transition.effect)
    }

    @Test
    fun aRefreshWhileLoadingIsIgnored() {
        val transition = reducer.start(state(refresh = LoadState.Loading), PagingAction.Refresh(RefreshMode.SOFT), null)

        assertNull(transition.effect)
    }

    @Test
    fun aReloadMergesTheFirstPage() {
        val transition = reducer.start(state(items = rows("a")), PagingAction.Reload, "next")

        assertEquals(PagingEffect.Load(PageRequest(null, 20), LoadMode.Merge), transition.effect)
        assertEquals(LoadState.Loading, transition.state.refresh)
    }

    @Test
    fun loadMoreUsesTheCursorAndAppends() {
        val transition = reducer.start(state(items = rows("a")), PagingAction.LoadMore, "cursor-1")

        assertEquals(PagingEffect.Load(PageRequest("cursor-1", 20), LoadMode.Append), transition.effect)
        assertEquals(LoadState.Loading, transition.state.append)
    }

    @Test
    fun loadMoreIsIgnoredWithoutItemsAndAtTheEnd() {
        assertNull(reducer.start(state(), PagingAction.LoadMore, null).effect)
        assertNull(reducer.start(state(items = rows("a"), append = LoadState.End), PagingAction.LoadMore, "x").effect)
        assertNull(reducer.start(state(items = rows("a"), append = LoadState.Loading), PagingAction.LoadMore, "x").effect)
    }

    @Test
    fun retryRestartsTheFailedPartOnly() {
        val refresh = reducer.start(state(refresh = LoadState.Failed(failure)), PagingAction.Retry, null)
        assertEquals(PagingEffect.Load(PageRequest(null, 20), LoadMode.Reset), refresh.effect)

        val append = reducer.start(state(items = rows("a"), append = LoadState.Failed(failure)), PagingAction.Retry, "next")
        assertEquals(PagingEffect.Load(PageRequest("next", 20), LoadMode.Append), append.effect)

        assertNull(reducer.start(state(items = rows("a")), PagingAction.Retry, "next").effect)
    }

    @Test
    fun aLoadedResetPageReplacesTheListAndEnds() {
        val loaded = reducer.loaded(state(items = rows("old")), Page(rows("a", "b"), null), LoadMode.Reset)

        assertEquals(listOf("a", "b"), loaded.items.map { it.key })
        assertEquals(LoadState.End, loaded.refresh)
        assertEquals(LoadState.Idle, loaded.append)
    }

    @Test
    fun aLoadedAppendPageKeepsThePreviousItems() {
        val loaded = reducer.loaded(state(items = rows("a")), Page(rows("a", "b"), "next"), LoadMode.Append)

        assertEquals(listOf("a", "b"), loaded.items.map { it.key })
        assertEquals(LoadState.Idle, loaded.append)
    }

    @Test
    fun aMergeKeepsTheLoadedTailAndUpdatesTheFirstPage() {
        val loaded = reducer.loaded(
            state(items = rows("a", "b", "c")),
            Page(rows("fresh", "a"), null),
            LoadMode.Merge,
        )

        assertEquals(listOf("fresh", "a", "b", "c"), loaded.items.map { it.key })
        assertEquals(LoadState.End, loaded.refresh)
    }

    @Test
    fun aMergeWithMorePagesKeepsAppendingPossible() {
        val loaded = reducer.loaded(
            state(items = rows("a", "b", "c"), append = LoadState.End),
            Page(rows("fresh", "a"), "next"),
            LoadMode.Merge,
        )

        assertEquals(LoadState.Idle, loaded.append)
    }

    @Test
    fun aFailedRefreshKeepsTheItemsAndExposesTheFailure() {
        val failed = reducer.failed(state(items = rows("a")), failure, LoadMode.Reset)

        assertEquals(listOf("a"), failed.items.map { it.key })
        assertEquals(LoadState.Failed(failure), failed.refresh)
    }

    @Test
    fun aFailedAppendDoesNotTouchTheItems() {
        val failed = reducer.failed(state(items = rows("a")), failure, LoadMode.Append)

        assertEquals(listOf("a"), failed.items.map { it.key })
        assertEquals(LoadState.Failed(failure), failed.append)
        assertTrue(failed.items.isNotEmpty())
    }

    private fun state(
        items: List<Row> = emptyList(),
        refresh: LoadState = LoadState.Idle,
        append: LoadState = LoadState.Idle,
    ) = PagedState(items, refresh, append)

    private data class Row(override val key: String) : Keyed

    private companion object {
        val failure = RemoteFailure.Offline("offline")

        fun rows(vararg keys: String): List<Row> = keys.map(::Row)
    }
}
