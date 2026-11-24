package dev.proflyder.home.feature.settings.presentation

import dev.proflyder.home.core.access.AppAccessSource
import dev.proflyder.home.core.access.AppAccessState
import dev.proflyder.home.core.model.AppAccess
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.runtime.AppVersion
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class SettingsViewModelTest {
    private class FakeAccess(initial: AppAccessState) : AppAccessSource {
        val mutable = MutableStateFlow(initial)
        override val state: StateFlow<AppAccessState> = mutable
    }

    private object Version : AppVersion {
        override val name: String = "test"
        override val debuggable: Boolean = false
    }

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun onlyLinksWithTheirModuleRemain() = runTest(UnconfinedTestDispatcher()) {
        val access = FakeAccess(
            AppAccessState.Ready(
                AppAccess(
                    name = "Phone",
                    modules = setOf(AppModule.BASE, AppModule.HOME, AppModule.NOTIFICATIONS),
                    owner = false,
                ),
            ),
        )
        val viewModel = SettingsViewModel(Version, access)

        assertEquals(
            setOf(
                SettingsLink.CONNECTION,
                SettingsLink.NOTIFICATIONS,
                SettingsLink.APPEARANCE,
                SettingsLink.DESIGN_SYSTEM,
            ),
            viewModel.state.value.links,
        )

        access.mutable.value = AppAccessState.Unknown
        assertTrue(viewModel.state.value.links.contains(SettingsLink.VPN))
        assertTrue(viewModel.state.value.links.contains(SettingsLink.DESIGN_SYSTEM))
        assertFalse(viewModel.state.value.designSystemVisible)
    }
}
