package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.FavoriteKind
import dev.proflyder.home.core.model.FavoritePreferences
import dev.proflyder.home.core.model.FavoriteRef
import dev.proflyder.home.core.model.Favorites
import dev.proflyder.home.feature.control.ControlCatalogState
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ControlFavoritesTest {
    private class FakePreferences(initial: List<FavoriteRef>?) : FavoritePreferences {
        val stored = MutableStateFlow(initial)

        override val favorites = stored

        override suspend fun saveIfAbsent(refs: List<FavoriteRef>) {
            if (stored.value == null) stored.value = refs
        }

        override suspend fun toggle(ref: FavoriteRef): Boolean {
            stored.value = Favorites.toggle(stored.value.orEmpty(), ref)
            return ref in stored.value.orEmpty()
        }
    }

    private val speak = ControlEntry(
        id = "action.speak",
        kind = ControlKind.ACTION,
        title = "Speak",
        room = "",
        icon = "",
        actions = setOf(ControlAction.RUN),
        power = null,
        available = true,
    )
    private val loaded = ControlCatalogState(catalog = ControlCatalog(listOf(speak), statesOk = true))

    @Test
    fun storedFavoritesWinAndAreNotReseeded() = runBlocking {
        val preferences = FakePreferences(emptyList())
        val refs = ControlFavorites(preferences).resolved(flowOf(loaded), flowOf(emptyList())).first()
        assertEquals(emptyList<FavoriteRef>(), refs)
    }

    @Test
    fun defaultsAreSeededOnceTheCatalogAndScenariosAreKnown() = runBlocking {
        val preferences = FakePreferences(null)
        val favorites = ControlFavorites(preferences)
        assertEquals(emptyList<FavoriteRef>(), favorites.resolved(flowOf(ControlCatalogState()), flowOf(emptyList())).first())
        assertNull(preferences.stored.value)
        val refs = favorites.resolved(flowOf(loaded), flowOf(emptyList())).first()
        assertEquals(listOf(FavoriteRef(FavoriteKind.ACTION, speak.id)), refs)
        assertEquals(refs, preferences.stored.value)
    }

    @Test
    fun togglingReportsTheNewPinState() = runBlocking {
        val preferences = FakePreferences(emptyList())
        val subject = PinSubject.Entry(speak, FavoriteRef(FavoriteKind.ACTION, speak.id))
        val event = ControlFavorites(preferences).toggle(subject)
        assertEquals(ControlEvent.Notice(ControlNotice.FavoriteChanged(subject, pinned = true)), event)
        assertTrue(subject.ref in preferences.stored.value.orEmpty())
    }
}
