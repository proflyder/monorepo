package dev.proflyder.home.feature.settings.presentation

import dev.proflyder.home.core.access.AppAccessSource
import dev.proflyder.home.core.access.AppAccessState
import dev.proflyder.home.core.model.AppAccess
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.runtime.AppVersion
import dev.proflyder.home.feature.update.AvailableUpdate
import dev.proflyder.home.feature.update.UpdateCenter
import dev.proflyder.home.feature.update.UpdateFailureKind
import dev.proflyder.home.feature.update.UpdatePhase
import dev.proflyder.home.feature.update.UpdateState
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

    private class FakeUpdates(
        override val supported: Boolean = true,
        initial: UpdateState = UpdateState(currentVersion = VERSION),
    ) : UpdateCenter {
        val mutable = MutableStateFlow(initial)
        override val state: StateFlow<UpdateState> = mutable
        var checks = 0

        override fun checkNow() {
            checks += 1
        }

        override fun signIn() = Unit

        override fun install() = Unit

        override fun dismissPrompt() = Unit
    }

    private object Version : AppVersion {
        override val name: String = VERSION
        override val debuggable: Boolean = false
    }

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun unknownAccess(): FakeAccess = FakeAccess(AppAccessState.Unknown)

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
        val viewModel = SettingsViewModel(Version, access, FakeUpdates())

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

    @Test
    fun theUpdateRowShowsTheVersionAndAsksForACheck() = runTest(UnconfinedTestDispatcher()) {
        val updates = FakeUpdates()
        val viewModel = SettingsViewModel(Version, unknownAccess(), updates)

        assertEquals(SettingsUpdateRow(visible = true, version = VERSION), viewModel.state.value.update)

        viewModel.dispatch(SettingsIntent.CheckUpdates)

        assertEquals(1, updates.checks)
    }

    @Test
    fun theUpdateRowFollowsTheAvailableReleaseAndErrors() = runTest(UnconfinedTestDispatcher()) {
        val updates = FakeUpdates()
        val viewModel = SettingsViewModel(Version, unknownAccess(), updates)

        updates.mutable.value = UpdateState(
            currentVersion = VERSION,
            phase = UpdatePhase.AVAILABLE,
            available = AvailableUpdate("0.2.0", 2L, "notes"),
        )
        assertEquals(SettingsUpdateStatus.AVAILABLE, viewModel.state.value.update.status)
        assertEquals("0.2.0", viewModel.state.value.update.latestVersion)

        updates.mutable.value = UpdateState(
            currentVersion = VERSION,
            phase = UpdatePhase.FAILED,
            failure = UpdateFailureKind.OFFLINE,
        )
        assertEquals(SettingsUpdateStatus.OFFLINE, viewModel.state.value.update.status)

        updates.mutable.value = UpdateState(currentVersion = VERSION, phase = UpdatePhase.UP_TO_DATE)
        assertEquals(SettingsUpdateStatus.UP_TO_DATE, viewModel.state.value.update.status)
    }

    @Test
    fun aBuildWithoutTheSdkHidesTheUpdateRow() = runTest(UnconfinedTestDispatcher()) {
        val viewModel = SettingsViewModel(Version, unknownAccess(), FakeUpdates(supported = false))

        assertFalse(viewModel.state.value.update.visible)
    }
}

private const val VERSION = "0.1.0-test"
