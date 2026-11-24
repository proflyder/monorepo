package dev.proflyder.home.feature.secrets

import dev.proflyder.home.core.model.DeviceEnrollment
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.feature.secrets.data.SecretsUnlockSession
import dev.proflyder.home.feature.secrets.presentation.SecretsCatalogIntent
import dev.proflyder.home.feature.secrets.presentation.SecretsCatalogViewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class SecretsCatalogViewModelTest {
    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun theModuleStaysLockedAndLoadsNothingUntilAuthenticationSucceeds() {
        val repository = FakeSecretsRepository()
        val unlock = SecretsUnlockSession()
        val viewModel = SecretsCatalogViewModel(repository, unlock)

        assertTrue(viewModel.state.value.locked)
        assertEquals(0, repository.refreshes)

        viewModel.dispatch(SecretsCatalogIntent.AuthenticationFinished(false))
        assertTrue(viewModel.state.value.locked)
        assertEquals(0, repository.refreshes)

        viewModel.dispatch(SecretsCatalogIntent.AuthenticationFinished(true))
        assertFalse(viewModel.state.value.locked)
        assertEquals(1, repository.refreshes)
    }

    @Test
    fun pathsAreGroupedAndSearchMatchesPathOrDescription() {
        val repository = FakeSecretsRepository()
        repository.catalogState.value = SecretsCatalogState(
            loading = false,
            entries = listOf(
                FakeSecretsRepository.entry("k8s/test", "probe"),
                FakeSecretsRepository.entry("k8s/home-assistant/phone-push", "push tokens"),
                FakeSecretsRepository.entry("infra/backup/offsite", "restic"),
            ),
            truncated = false,
        )
        val viewModel = SecretsCatalogViewModel(repository, SecretsUnlockSession().apply { unlock() })

        assertEquals(listOf("infra", "k8s"), viewModel.state.value.groups.map { it.name })
        assertEquals(2, viewModel.state.value.groups.last().entries.size)

        viewModel.dispatch(SecretsCatalogIntent.Query("push"))
        assertEquals(listOf("k8s"), viewModel.state.value.groups.map { it.name })
        assertEquals(1, viewModel.state.value.groups.single().entries.size)

        viewModel.dispatch(SecretsCatalogIntent.Query("restic"))
        assertEquals(listOf("infra"), viewModel.state.value.groups.map { it.name })
    }

    @Test
    fun theCatalogShowsTheBindCardOnlyForAPhoneThatIsNotBound() {
        val repository = FakeSecretsRepository()
        repository.catalogState.value = SecretsCatalogState(
            loading = false,
            entries = listOf(FakeSecretsRepository.entry("k8s/home-assistant/phone-push", "push tokens")),
        )
        val viewModel = SecretsCatalogViewModel(repository, SecretsUnlockSession().apply { unlock() })

        assertFalse(viewModel.state.value.deviceNotEnrolled)
        assertFalse(viewModel.state.value.deviceEnrolled)

        viewModel.dispatch(SecretsCatalogIntent.RefreshDevice)
        assertTrue(viewModel.state.value.deviceNotEnrolled)
        assertFalse(viewModel.state.value.deviceEnrolled)

        repository.deviceResult = Outcome.Success(DeviceEnrollment(true, "device-1", 1))
        viewModel.dispatch(SecretsCatalogIntent.RefreshDevice)
        assertFalse(viewModel.state.value.deviceNotEnrolled)
        assertTrue(viewModel.state.value.deviceEnrolled)
    }

    @Test
    fun theCatalogHidesTheBindCardUntilTheSecretsAreUnlocked() {
        val repository = FakeSecretsRepository()
        val viewModel = SecretsCatalogViewModel(repository, SecretsUnlockSession())

        viewModel.dispatch(SecretsCatalogIntent.RefreshDevice)

        assertFalse(viewModel.state.value.deviceNotEnrolled)
        assertTrue(viewModel.state.value.locked)
    }
}
