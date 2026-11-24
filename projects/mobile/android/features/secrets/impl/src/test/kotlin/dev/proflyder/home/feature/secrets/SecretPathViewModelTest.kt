package dev.proflyder.home.feature.secrets

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SecretOperation
import dev.proflyder.home.feature.secrets.presentation.SecretAction
import dev.proflyder.home.feature.secrets.presentation.SecretActionKind
import dev.proflyder.home.feature.secrets.presentation.SecretDialog
import dev.proflyder.home.feature.secrets.presentation.SecretPathIntent
import dev.proflyder.home.feature.secrets.presentation.SecretPathViewModel
import dev.proflyder.home.feature.secrets.presentation.SecretProblem
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class SecretPathViewModelTest {
    private val dispatcher = StandardTestDispatcher()

    @Before
    fun setUp() = Dispatchers.setMain(dispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    private fun viewModel(repository: FakeSecretsRepository): SecretPathViewModel {
        val viewModel = SecretPathViewModel(repository)
        viewModel.dispatch(SecretPathIntent.SetPath(FakeSecretsRepository.PATH))
        return viewModel
    }

    private fun sign(viewModel: SecretPathViewModel, signature: String = "signature-1") {
        viewModel.dispatch(SecretPathIntent.StepUpSigned(signature))
    }

    @Test
    fun aRevealedValueIsMaskedAfterThirtySeconds() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        repository.stepUpState.value = true
        val viewModel = viewModel(repository)
        advanceUntilIdle()

        viewModel.dispatch(SecretPathIntent.Reveal("token"))
        runCurrent()

        assertEquals("token", viewModel.state.value.revealedKey)
        assertEquals("s3cret", viewModel.state.value.revealedValue)

        advanceTimeBy(30_001)
        runCurrent()

        assertNull(viewModel.state.value.revealedKey)
        assertNull(viewModel.state.value.revealedValue)
    }

    @Test
    fun revealWithoutASessionAsksToConfirmThePhoneFirst() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        val viewModel = viewModel(repository)
        advanceUntilIdle()

        viewModel.dispatch(SecretPathIntent.Reveal("token"))
        runCurrent()

        val dialog = viewModel.state.value.dialog as SecretDialog.StepUp
        assertEquals(SecretActionKind.REVEAL, dialog.action.kind)
        assertTrue(repository.revealed.isEmpty())

        viewModel.dispatch(SecretPathIntent.StepUpSubmit)
        advanceUntilIdle()
        assertEquals(listOf(SecretOperation.SESSION), repository.challenges)

        sign(viewModel)
        runCurrent()

        assertEquals("signature-1", repository.lastSignature)
        assertEquals("token", viewModel.state.value.revealedKey)
        assertEquals(1, repository.revealed.size)
    }

    @Test
    fun anExpiredSessionStopsAllowingRevealAndAsksForConfirmationAgain() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        repository.stepUpState.value = true
        val viewModel = viewModel(repository)
        advanceUntilIdle()
        viewModel.dispatch(SecretPathIntent.Reveal("token"))
        runCurrent()
        assertEquals("token", viewModel.state.value.revealedKey)

        viewModel.dispatch(SecretPathIntent.ReMask)
        repository.stepUpState.value = false
        runCurrent()
        viewModel.dispatch(SecretPathIntent.Reveal("endpoint"))
        runCurrent()

        assertTrue(viewModel.state.value.dialog is SecretDialog.StepUp)
    }

    @Test
    fun updateSignsTheOperationAndSendsTheCurrentVersionAsCas() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        val viewModel = viewModel(repository)
        advanceUntilIdle()

        viewModel.dispatch(SecretPathIntent.EditRequest("token"))
        viewModel.dispatch(SecretPathIntent.EditValue("new-value"))
        viewModel.dispatch(SecretPathIntent.EditSubmit)
        runCurrent()
        viewModel.dispatch(SecretPathIntent.StepUpSubmit)
        advanceUntilIdle()

        assertEquals(listOf(SecretOperation.UPDATE), repository.challenges)
        assertEquals(dev.proflyder.home.feature.secrets.presentation.sha256Hex("new-value"), repository.lastValueHash)
        assertEquals(emptyList<Triple<String, String, Long>>(), repository.updates)

        sign(viewModel)
        advanceUntilIdle()

        assertEquals(listOf(Triple("token", "new-value", 3L)), repository.updates)
        assertNull(viewModel.state.value.dialog)
    }

    @Test
    fun rollbackConfirmsThenSignsTheSelectedVersion() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        val viewModel = viewModel(repository)
        advanceUntilIdle()

        viewModel.dispatch(SecretPathIntent.RollbackRequest(1))
        runCurrent()
        assertEquals(
            SecretDialog.Confirm(SecretAction(SecretActionKind.ROLLBACK, version = 1)),
            viewModel.state.value.dialog,
        )

        viewModel.dispatch(SecretPathIntent.ConfirmAccept)
        runCurrent()
        viewModel.dispatch(SecretPathIntent.StepUpSubmit)
        advanceUntilIdle()
        assertEquals(listOf(SecretOperation.ROLLBACK), repository.challenges)

        sign(viewModel)
        advanceUntilIdle()

        assertEquals(listOf(1L to 3L), repository.rollbacks)
    }

    @Test
    fun deleteSignsTheCurrentAndExpectedVersion() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        val viewModel = viewModel(repository)
        advanceUntilIdle()

        viewModel.dispatch(SecretPathIntent.DeleteRequest)
        runCurrent()
        viewModel.dispatch(SecretPathIntent.ConfirmAccept)
        runCurrent()
        viewModel.dispatch(SecretPathIntent.StepUpSubmit)
        advanceUntilIdle()
        assertEquals(listOf(SecretOperation.DELETE), repository.challenges)

        sign(viewModel)
        advanceUntilIdle()

        assertEquals(listOf(3L to 3L), repository.deletions)
    }

    @Test
    fun aRejectedSignatureKeepsTheDialogAndShowsItsProblem() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        repository.updateResult = Outcome.Failure(
            RemoteFailure.Server(403, dev.proflyder.home.core.model.ConnectCode.UNKNOWN, "step_up_invalid", "step_up_invalid"),
        )
        val viewModel = viewModel(repository)
        advanceUntilIdle()

        viewModel.dispatch(SecretPathIntent.EditRequest("token"))
        viewModel.dispatch(SecretPathIntent.EditSubmit)
        runCurrent()
        viewModel.dispatch(SecretPathIntent.StepUpSubmit)
        advanceUntilIdle()
        sign(viewModel)
        advanceUntilIdle()

        val dialog = viewModel.state.value.dialog as SecretDialog.StepUp
        assertEquals(SecretProblem.InvalidSignature, dialog.problem)
        assertEquals(false, dialog.busy)
    }

    @Test
    fun cancellingTheSystemPromptKeepsTheDialogForAnotherAttempt() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        val viewModel = viewModel(repository)
        advanceUntilIdle()

        viewModel.dispatch(SecretPathIntent.Reveal("token"))
        runCurrent()
        viewModel.dispatch(SecretPathIntent.StepUpSubmit)
        advanceUntilIdle()
        viewModel.dispatch(SecretPathIntent.StepUpCancelled)
        runCurrent()

        val dialog = viewModel.state.value.dialog as SecretDialog.StepUp
        assertEquals(false, dialog.busy)
        assertTrue(repository.revealed.isEmpty())
    }

    @Test
    fun anUnsupportedPhoneClosesTheDialogWithADeviceProblem() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        val viewModel = viewModel(repository)
        advanceUntilIdle()

        viewModel.dispatch(SecretPathIntent.Reveal("token"))
        runCurrent()
        viewModel.dispatch(
            SecretPathIntent.StepUpFailed(dev.proflyder.home.feature.secrets.presentation.DeviceSignatureFailure.UNSUPPORTED),
        )
        runCurrent()

        val dialog = viewModel.state.value.dialog as SecretDialog.StepUp
        assertEquals(SecretProblem.DeviceUnavailable, dialog.problem)
    }
}
