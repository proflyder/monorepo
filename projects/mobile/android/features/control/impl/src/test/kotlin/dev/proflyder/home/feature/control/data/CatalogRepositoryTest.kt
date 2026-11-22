package dev.proflyder.home.feature.control.data

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.config.TokenStatus
import dev.proflyder.home.core.endpoint.EndpointParseResult
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.ControlAction
import dev.proflyder.home.core.model.ControlCatalog
import dev.proflyder.home.core.model.ControlEntry
import dev.proflyder.home.core.model.ControlKind
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.PowerState
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.network.ControlCatalogClient
import dev.proflyder.home.core.network.SchedulerClient
import dev.proflyder.home.core.runtime.AppForeground
import dev.proflyder.home.core.runtime.ForegroundState
import dev.proflyder.home.core.testing.FakeClock
import java.lang.reflect.Proxy
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class CatalogRepositoryTest {
    private val lamp = ControlEntry(
        id = "light.lamp",
        kind = ControlKind.LIGHT,
        title = "Lamp",
        room = "",
        icon = "",
        actions = setOf(ControlAction.TURN_ON, ControlAction.TURN_OFF),
        power = PowerState.OFF,
        available = true,
    )

    private class FakeSettings(configured: Boolean) : ConnectionSettingsStore {
        override val config = MutableStateFlow(ConnectionConfig(controlEndpoint = endpoint().takeIf { configured }))
        override val tokenStatus = MutableStateFlow(TokenStatus.ABSENT)

        override suspend fun updateToken(bearerToken: String?): Outcome<Unit> = Outcome.Success(Unit)
    }

    private class FakeCatalog(var entries: List<ControlEntry>) : ControlCatalogClient {
        var fetches = 0
        var failure: RemoteFailure? = null

        override suspend fun fetch(): Outcome<ControlCatalog> {
            fetches++
            return failure?.let { Outcome.Failure(it) } ?: Outcome.Success(ControlCatalog(entries, statesOk = true))
        }
    }

    private class FakeForeground(visible: Boolean) : AppForeground {
        override val state = MutableStateFlow(ForegroundState(visible, 0L))
    }

    private var enqueueResult: Outcome<String> = Outcome.Success("task")

    private val scheduler = Proxy.newProxyInstance(
        SchedulerClient::class.java.classLoader,
        arrayOf(SchedulerClient::class.java),
    ) { _, method, _ -> if (method.name == "enqueueTask") enqueueResult else error("unexpected ${method.name}") } as SchedulerClient

    private fun TestScope.repository(
        catalog: FakeCatalog,
        foreground: FakeForeground = FakeForeground(visible = true),
        configured: Boolean = true,
    ) = CatalogRepository(
        settings = FakeSettings(configured),
        foreground = foreground,
        catalogClient = catalog,
        scheduler = scheduler,
        optimistic = OptimisticDeviceState(FakeClock(), ttlNanos = Long.MAX_VALUE / 2),
        policy = ControlRefreshPolicy(),
        scope = backgroundScope,
    )

    @Test
    fun theCatalogLoadsWhileForegroundAndRefreshesOnThePolicyPeriod() = runTest {
        val catalog = FakeCatalog(listOf(lamp))
        val repository = repository(catalog)
        backgroundScope.launch { repository.catalog.collect {} }
        runCurrent()
        assertEquals(listOf(lamp), repository.catalog.value.entries)
        assertFalse(repository.catalog.value.loading)
        advanceTimeBy(ControlRefreshPolicy().period.inWholeMilliseconds + 1)
        assertEquals(2, catalog.fetches)
    }

    @Test
    fun inTheBackgroundOnlyAnExplicitRefreshFetches() = runTest {
        val catalog = FakeCatalog(listOf(lamp))
        val repository = repository(catalog, FakeForeground(visible = false))
        backgroundScope.launch { repository.catalog.collect {} }
        advanceTimeBy(ControlRefreshPolicy().period.inWholeMilliseconds * 2)
        assertEquals(0, catalog.fetches)
        repository.refresh()
        runCurrent()
        assertEquals(1, catalog.fetches)
    }

    @Test
    fun withoutAControlEndpointTheCatalogIsUnconfigured() = runTest {
        val catalog = FakeCatalog(listOf(lamp))
        val repository = repository(catalog, configured = false)
        backgroundScope.launch { repository.catalog.collect {} }
        runCurrent()
        assertFalse(repository.catalog.value.configured)
        assertEquals(0, catalog.fetches)
    }

    @Test
    fun aPowerCommandShowsTheExpectedStateUntilTheServerAgrees() = runTest {
        val catalog = FakeCatalog(listOf(lamp))
        val repository = repository(catalog)
        backgroundScope.launch { repository.catalog.collect {} }
        runCurrent()
        repository.send(lamp.id, ControlAction.TURN_ON)
        runCurrent()
        assertEquals(PowerState.ON, repository.catalog.value.entries.single().power)
        advanceTimeBy(ControlRefreshPolicy().powerSettle.first().inWholeMilliseconds + 1)
        assertTrue("settle refresh polled the server", catalog.fetches >= 2)
        assertEquals(PowerState.ON, repository.catalog.value.entries.single().power)
        catalog.entries = listOf(lamp.copy(power = PowerState.ON))
        repository.refresh()
        runCurrent()
        val fetchesWhenSettled = catalog.fetches
        advanceTimeBy(ControlRefreshPolicy().powerSettle.first().inWholeMilliseconds * 2)
        assertEquals("no more settle polls once the server agrees", fetchesWhenSettled, catalog.fetches)
    }

    @Test
    fun aFailedCommandRollsTheExpectationBack() = runTest {
        val catalog = FakeCatalog(listOf(lamp))
        val repository = repository(catalog)
        backgroundScope.launch { repository.catalog.collect {} }
        runCurrent()
        enqueueResult = Outcome.Failure(RemoteFailure.Timeout)
        val result = repository.send(lamp.id, ControlAction.TURN_ON)
        runCurrent()
        assertTrue(result is Outcome.Failure)
        assertEquals(PowerState.OFF, repository.catalog.value.entries.single().power)
    }

    @Test
    fun aFailedReloadKeepsTheLastCatalogAndReportsTheFailure() = runTest {
        val catalog = FakeCatalog(listOf(lamp))
        val repository = repository(catalog)
        backgroundScope.launch { repository.catalog.collect {} }
        runCurrent()
        catalog.failure = RemoteFailure.Timeout
        repository.refresh()
        runCurrent()
        assertEquals(listOf(lamp), repository.catalog.value.entries)
        assertEquals(RemoteFailure.Timeout, repository.catalog.value.failure)
    }

    private companion object {
        fun endpoint(): ServerEndpoint =
            (ServerEndpoint.parse("https://control.example.com") as EndpointParseResult.Valid).endpoint
    }
}
