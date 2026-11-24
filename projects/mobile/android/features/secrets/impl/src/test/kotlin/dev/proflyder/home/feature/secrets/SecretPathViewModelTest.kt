package dev.proflyder.home.feature.secrets

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.feature.secrets.presentation.SecretActionKind
import dev.proflyder.home.feature.secrets.presentation.SecretDialog
import dev.proflyder.home.feature.secrets.presentation.SecretPathIntent
import dev.proflyder.home.feature.secrets.presentation.SecretPathViewModel
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
    fun revealWithoutASessionAsksForATotpCodeFirst() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        val viewModel = viewModel(repository)
        advanceUntilIdle()

        viewModel.dispatch(SecretPathIntent.Reveal("token"))
        runCurrent()

        val dialog = viewModel.state.value.dialog as SecretDialog.StepUp
        assertEquals(SecretActionKind.REVEAL, dialog.action.kind)
        assertTrue(repository.revealed.isEmpty())

        viewModel.dispatch(SecretPathIntent.StepUpDigits("123456"))
        viewModel.dispatch(SecretPathIntent.StepUpSubmit)
        runCurrent()

        assertEquals("123456", repository.lastTotp)
        assertEquals("token", viewModel.state.value.revealedKey)
        assertEquals(1, repository.revealed.size)
    }

    @Test
    fun anExpiredSessionStopsAllowingRevealAndAsksForACodeAgain() = runTest(dispatcher.scheduler) {
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
    fun updateNeedsAFreshCodeEveryTimeAndSendsTheCurrentVersionAsCas() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        val viewModel = viewModel(repository)
        advanceUntilIdle()

        viewModel.dispatch(SecretPathIntent.EditRequest("token"))
        viewModel.dispatch(SecretPathIntent.EditValue("new-value"))
        viewModel.dispatch(SecretPathIntent.EditSubmit)
        runCurrent()
        viewModel.dispatch(SecretPathIntent.StepUpDigits("654321"))
        viewModel.dispatch(SecretPathIntent.StepUpSubmit)
        advanceUntilIdle()

        assertEquals(listOf(Triple("token", "new-value", 3L)), repository.updates)
        assertEquals("654321", repository.lastTotp)
        assertNull(viewModel.state.value.dialog)
    }

    @Test
    fun rollbackConfirmsBeforeItAsksForACodeAndUsesTheSelectedVersion() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        val viewModel = viewModel(repository)
        advanceUntilIdle()

        viewModel.dispatch(SecretPathIntent.RollbackRequest(1))
        runCurrent()
        assertEquals(SecretDialog.Confirm(dev.proflyder.home.feature.secrets.presentation.SecretAction(SecretActionKind.ROLLBACK, version = 1)), viewModel.state.value.dialog)

        viewModel.dispatch(SecretPathIntent.ConfirmAccept)
        runCurrent()
        viewModel.dispatch(SecretPathIntent.StepUpDigits("111111"))
        viewModel.dispatch(SecretPathIntent.StepUpSubmit)
        advanceUntilIdle()

        assertEquals(listOf(1L to 3L), repository.rollbacks)
    }

    @Test
    fun deleteSendsTheCurrentAndExpectedVersion() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        val viewModel = viewModel(repository)
        advanceUntilIdle()

        viewModel.dispatch(SecretPathIntent.DeleteRequest)
        runCurrent()
        viewModel.dispatch(SecretPathIntent.ConfirmAccept)
        runCurrent()
        viewModel.dispatch(SecretPathIntent.StepUpDigits("222222"))
        viewModel.dispatch(SecretPathIntent.StepUpSubmit)
        advanceUntilIdle()

        assertEquals(listOf(3L to 3L), repository.deletions)
    }

    @Test
    fun anInvalidCodeKeepsTheDialogAndShowsItsProblem() = runTest(dispatcher.scheduler) {
        val repository = FakeSecretsRepository()
        repository.updateResult = Outcome.Failure(
            RemoteFailure.Server(403, dev.proflyder.home.core.model.ConnectCode.UNKNOWN, "step_up_invalid", "step_up_invalid"),
        )
        val viewModel = viewModel(repository)
        advanceUntilIdle()

        viewModel.dispatch(SecretPathIntent.EditRequest("token"))
        viewModel.dispatch(SecretPathIntent.EditSubmit)
        runCurrent()
        viewModel.dispatch(SecretPathIntent.StepUpDigits("999999"))
        viewModel.dispatch(SecretPathIntent.StepUpSubmit)
        advanceUntilIdle()

        val dialog = viewModel.state.value.dialog as SecretDialog.StepUp
        assertEquals(dev.proflyder.home.feature.secrets.presentation.SecretProblem.InvalidTotp, dialog.problem)
        assertEquals("", dialog.digits)
    }
}
