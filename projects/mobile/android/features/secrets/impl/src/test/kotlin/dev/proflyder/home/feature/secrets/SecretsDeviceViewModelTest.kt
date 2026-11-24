package dev.proflyder.home.feature.secrets

import dev.proflyder.home.core.model.DeviceAttestationChallenge
import dev.proflyder.home.core.model.DeviceEnrollment
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.feature.secrets.presentation.DeviceKeyMaterial
import dev.proflyder.home.feature.secrets.presentation.SecretProblem
import dev.proflyder.home.feature.secrets.presentation.SecretsDeviceEvent
import dev.proflyder.home.feature.secrets.presentation.SecretsDeviceIntent
import dev.proflyder.home.feature.secrets.presentation.SecretsDeviceViewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
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
class SecretsDeviceViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @Before
    fun setUp() = Dispatchers.setMain(dispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun theScreenReportsTheDeviceAndAsksForAnAttestationKey() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        val viewModel = SecretsDeviceViewModel(repository)
        advanceUntilIdle()

        val event = viewModel.screenEvents.next.first()
        assertEquals(SecretsDeviceEvent.CreateKey("attestation-challenge"), event.event)
        assertFalse(viewModel.state.value.enrolled)
    }

    @Test
    fun anEnrolledDeviceKeepsItsIdentifier() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        repository.deviceResult = Outcome.Success(DeviceEnrollment(true, "device-7", 10))
        val viewModel = SecretsDeviceViewModel(repository)
        advanceUntilIdle()

        assertTrue(viewModel.state.value.enrolled)
        assertEquals("device-7", viewModel.state.value.deviceId)
    }

    @Test
    fun anEnrolledDeviceDoesNotRegenerateTheKeyWhenTheScreenOpens() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        repository.deviceResult = Outcome.Success(DeviceEnrollment(true, "device-7", 10))
        val viewModel = SecretsDeviceViewModel(repository)
        advanceUntilIdle()

        assertEquals(0, repository.deviceChallenges)
        assertFalse(viewModel.state.value.keyReady)
        assertFalse(viewModel.state.value.rebinding)
    }

    @Test
    fun anEnrolledDeviceAsksToReconcileTheStagedKey() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        repository.deviceResult = Outcome.Success(DeviceEnrollment(true, "device-7", 10, "public-key"))
        val viewModel = SecretsDeviceViewModel(repository)
        advanceUntilIdle()

        val event = viewModel.screenEvents.next.first()
        assertEquals(SecretsDeviceEvent.ReconcileKey("public-key"), event.event)
        assertEquals(0, repository.deviceChallenges)
    }

    @Test
    fun rebindingStartsOnlyAfterAnExplicitIntent() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        repository.deviceResult = Outcome.Success(DeviceEnrollment(true, "device-7", 10))
        val viewModel = SecretsDeviceViewModel(repository)
        advanceUntilIdle()
        assertEquals(0, repository.deviceChallenges)

        viewModel.dispatch(SecretsDeviceIntent.Rebind)
        advanceUntilIdle()

        assertEquals(1, repository.deviceChallenges)
        assertTrue(viewModel.state.value.rebinding)
        val event = viewModel.screenEvents.next.first()
        assertEquals(SecretsDeviceEvent.CreateKey("attestation-challenge"), event.event)
    }

    @Test
    fun bindingAnEnrolledKeySendsTheNormalizedCodeAndMaterial() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        val viewModel = SecretsDeviceViewModel(repository)
        advanceUntilIdle()
        viewModel.dispatch(SecretsDeviceIntent.KeyReady(DeviceKeyMaterial("public-key", listOf("cert1", "cert2"))))
        viewModel.dispatch(SecretsDeviceIntent.Code(" ab-cd 12 "))
        viewModel.dispatch(SecretsDeviceIntent.Bind)
        advanceUntilIdle()

        assertEquals(
            listOf(listOf("ABCD12", "public-key", "attestation-challenge", "2")),
            repository.enrollments,
        )
        assertTrue(viewModel.state.value.enrolled)
        assertEquals("device-1", viewModel.state.value.deviceId)
        assertEquals("", viewModel.state.value.code)
    }

    @Test
    fun aRejectedCodeShowsTheEnrollmentProblem() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        repository.enrollResult = Outcome.Failure(
            RemoteFailure.Server(403, dev.proflyder.home.core.model.ConnectCode.UNKNOWN, "enrollment_invalid", "enrollment_invalid"),
        )
        val viewModel = SecretsDeviceViewModel(repository)
        advanceUntilIdle()
        viewModel.dispatch(SecretsDeviceIntent.KeyReady(DeviceKeyMaterial("public-key", listOf("cert"))))
        viewModel.dispatch(SecretsDeviceIntent.Code("ABCD12"))
        viewModel.dispatch(SecretsDeviceIntent.Bind)
        advanceUntilIdle()

        assertEquals(SecretProblem.EnrollmentRejected, viewModel.state.value.problem)
        assertFalse(viewModel.state.value.busy)
        assertFalse(viewModel.state.value.enrolled)
    }

    @Test
    fun anUnsupportedPhoneCannotBind() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        val viewModel = SecretsDeviceViewModel(repository)
        advanceUntilIdle()
        viewModel.dispatch(
            SecretsDeviceIntent.KeyFailed(dev.proflyder.home.feature.secrets.presentation.DeviceSignatureFailure.UNSUPPORTED),
        )
        viewModel.dispatch(SecretsDeviceIntent.Code("ABCD12"))
        viewModel.dispatch(SecretsDeviceIntent.Bind)
        advanceUntilIdle()

        assertFalse(viewModel.state.value.keyReady)
        assertEquals(SecretProblem.DeviceUnavailable, viewModel.state.value.problem)
        assertTrue(repository.enrollments.isEmpty())
    }

    @Test
    fun aChallengeFailureIsReportedBeforeAnyKeyExists() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        repository.deviceChallengeResult = Outcome.Failure(
            RemoteFailure.Server(503, dev.proflyder.home.core.model.ConnectCode.UNKNOWN, "device_unavailable", "device_unavailable"),
        )
        val viewModel = SecretsDeviceViewModel(repository)
        advanceUntilIdle()

        assertEquals(SecretProblem.DeviceUnavailable, viewModel.state.value.problem)
        assertFalse(viewModel.state.value.keyReady)
    }
}
