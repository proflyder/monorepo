package dev.proflyder.home.feature.control.presentation

import dev.proflyder.home.core.access.AppAccessSource
import dev.proflyder.home.core.access.AppAccessState
import dev.proflyder.home.core.model.AppAccess
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.navigation.Route
import dev.proflyder.home.feature.control.ProvidedTile
import dev.proflyder.home.feature.control.QuickTileAction
import dev.proflyder.home.feature.control.QuickTileProvider
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class QuickTileAccessTest {
    private class FakeAccess(initial: AppAccessState) : AppAccessSource {
        val mutable = MutableStateFlow(initial)
        override val state: StateFlow<AppAccessState> = mutable
    }

    private class FakeProvider(private val key: String, module: AppModule) : QuickTileProvider {
        override val tile = ProvidedTile(key, order = 0, labelRes = 0, iconRes = 0, module = module)

        override fun activate(): QuickTileAction = QuickTileAction.OpenFeature(key)
    }

    private val restricted = AppAccessState.Ready(
        AppAccess("Phone", setOf(AppModule.BASE, AppModule.HOME), owner = false),
    )

    private val everything = AppAccessState.Ready(
        AppAccess("Owner", AppModule.entries.toSet(), owner = true),
    )

    @Test
    fun callAndVpnTilesNeedTheirModules() {
        val access = FakeAccess(restricted)
        val launcher = QuickTileLauncher(
            setOf(FakeProvider("call", AppModule.INTERCOM_CALL), FakeProvider("vpn", AppModule.VPN)),
            access,
        )

        assertEquals(emptyList<String>(), launcher.tiles.map { it.key })
        assertNull(launcher.activate("call"))

        access.mutable.value = everything
        assertEquals(listOf("call", "vpn"), launcher.tiles.map { it.key })
        assertEquals(ControlEvent.OpenRoute(Route.Tab("call")), launcher.activate("call"))
    }

    @Test
    fun tilesWithoutAModuleStayVisibleWithBaseAccess() {
        val launcher = QuickTileLauncher(setOf(FakeProvider("voice", AppModule.BASE)), FakeAccess(restricted))

        assertEquals(listOf("voice"), launcher.tiles.map { it.key })
    }
}
