package dev.proflyder.home.feature.system.presentation

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SystemSnapshot
import dev.proflyder.home.feature.system.SystemFixtures
import dev.proflyder.home.feature.system.SystemRepository
import dev.proflyder.home.feature.system.data.combine
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class SystemViewModelTest {
    private class Repository(var next: Outcome<SystemSnapshot>) : SystemRepository {
        var calls = 0

        override suspend fun load(): Outcome<SystemSnapshot> {
            calls++
            return next
        }
    }

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun aFailedRefreshKeepsTheLastSnapshot() {
        val repository = Repository(Outcome.Success(SystemFixtures.snapshot))
        val viewModel = SystemViewModel(repository)
        viewModel.refresh()
        assertEquals(SystemFixtures.snapshot, viewModel.state.value.snapshot)
        assertFalse(viewModel.state.value.loading)

        repository.next = Outcome.Failure(RemoteFailure.Offline("down"))
        viewModel.refresh()
        assertEquals(SystemFixtures.snapshot, viewModel.state.value.snapshot)
        assertEquals(RemoteFailure.Offline("down"), viewModel.state.value.error)

        repository.next = Outcome.Success(SystemFixtures.empty)
        viewModel.refresh()
        assertNull(viewModel.state.value.error)
        assertEquals(3, repository.calls)
    }

    @Test
    fun combineReturnsTheFirstFailure() {
        val snapshot = SystemFixtures.snapshot
        val ok = combine(
            Outcome.Success(snapshot.summary),
            Outcome.Success(snapshot.hosts),
            Outcome.Success(snapshot.cluster),
            Outcome.Success(snapshot.devices),
        )
        assertEquals(Outcome.Success(snapshot), ok)
        val failed = combine(
            Outcome.Success(snapshot.summary),
            Outcome.Failure(RemoteFailure.Offline("down")),
            Outcome.Success(snapshot.cluster),
            Outcome.Success(snapshot.devices),
        )
        assertTrue(failed is Outcome.Failure)
    }
}
