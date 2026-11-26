package dev.proflyder.home.feature.access

import dev.proflyder.home.core.model.AccessToken
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.model.CreatedAccessToken
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.feature.access.impl.R
import dev.proflyder.home.feature.access.presentation.AccessEvent
import dev.proflyder.home.feature.access.presentation.AccessIntent
import dev.proflyder.home.feature.access.presentation.AccessViewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class AccessViewModelTest {
    private class FakeRepository : AccessRepository {
        val mutable = MutableStateFlow(AccessTokensState(loading = false, tokens = listOf(phone, owner)))
        var refreshes = 0
        var createResult: Outcome<CreatedAccessToken> = Outcome.Success(CreatedAccessToken("home_new", tablet))
        var updateResult: Outcome<AccessToken> = Outcome.Success(tablet)
        var revokeResult: Outcome<Unit> = Outcome.Success(Unit)
        var createdName: String? = null
        var createdModules: Set<AppModule>? = null

        override val state: StateFlow<AccessTokensState> = mutable

        override fun refresh() {
            refreshes++
        }

        override suspend fun create(name: String, modules: Set<AppModule>): Outcome<CreatedAccessToken> {
            createdName = name
            createdModules = modules
            return createResult
        }

        override suspend fun update(id: String, name: String, modules: Set<AppModule>): Outcome<AccessToken> =
            updateResult

        override suspend fun revoke(id: String): Outcome<Unit> = revokeResult
    }

    @Before
    fun setUp() = Dispatchers.setMain(UnconfinedTestDispatcher())

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun aNewTokenIsCreatedOnceAndItsPlainValueIsShownInTheDialog() = runTest(UnconfinedTestDispatcher()) {
        val repository = FakeRepository()
        val viewModel = AccessViewModel(repository)

        viewModel.dispatch(AccessIntent.New)
        viewModel.dispatch(AccessIntent.EditorName("  Планшет  "))
        viewModel.dispatch(AccessIntent.EditorModule(AppModule.HOME, true))
        viewModel.dispatch(AccessIntent.EditorModule(AppModule.NOTIFICATIONS, true))
        viewModel.dispatch(AccessIntent.EditorSave)

        assertEquals("Планшет", repository.createdName)
        assertEquals(setOf(AppModule.HOME, AppModule.NOTIFICATIONS), repository.createdModules)
        assertNull(viewModel.state.value.editor)
        assertEquals("home_new", viewModel.state.value.created?.token)
        assertEquals("Планшет", viewModel.state.value.created?.name)

        viewModel.dispatch(AccessIntent.CreatedDismiss)
        assertNull(viewModel.state.value.created)
    }

    @Test
    fun aTakenNameStaysInTheEditorWithALocalizedConflict() = runTest(UnconfinedTestDispatcher()) {
        val repository = FakeRepository()
        val conflict = RemoteFailure.Server(409, dev.proflyder.home.core.model.ConnectCode.UNKNOWN, "taken", "conflict")
        repository.createResult = Outcome.Failure(conflict)
        val viewModel = AccessViewModel(repository)

        viewModel.dispatch(AccessIntent.New)
        viewModel.dispatch(AccessIntent.EditorName("Owner"))
        viewModel.dispatch(AccessIntent.EditorSave)

        val editor = viewModel.state.value.editor
        assertEquals(R.string.access_error_name_taken, editor?.errorRes)
        assertFalse(editor?.saving == true)
        assertTrue(editor?.modules?.isEmpty() == true)

        viewModel.dispatch(AccessIntent.EditorName("Owner 2"))
        assertNull(viewModel.state.value.editor?.errorRes)
    }

    @Test
    fun anOwnerTokenCanBeRenamedButItsModulesStayFixed() = runTest(UnconfinedTestDispatcher()) {
        val repository = FakeRepository()
        val viewModel = AccessViewModel(repository)

        viewModel.dispatch(AccessIntent.Edit(owner))
        val editor = viewModel.state.value.editor
        assertEquals(owner.modules, editor?.modules)
        assertFalse(editor?.canEditModules == true)

        viewModel.dispatch(AccessIntent.EditorName("Owner 2"))
        viewModel.dispatch(AccessIntent.EditorModule(AppModule.CHAT, true))
        assertEquals("Owner 2", viewModel.state.value.editor?.name)
        assertEquals(owner.modules, viewModel.state.value.editor?.modules)
        assertTrue(viewModel.state.value.editor?.canSave == true)
    }

    @Test
    fun revokeAsksForConfirmationAndRefreshesTheListAfterSuccess() = runTest(UnconfinedTestDispatcher()) {
        val repository = FakeRepository()
        val viewModel = AccessViewModel(repository)
        val events = mutableListOf<AccessEvent>()
        backgroundScope.launch { viewModel.events.toList(events) }

        viewModel.dispatch(AccessIntent.Edit(owner))
        viewModel.dispatch(AccessIntent.EditorRevoke)
        assertTrue("the owner cannot revoke itself", events.isEmpty())
        viewModel.dispatch(AccessIntent.Edit(phone))
        viewModel.dispatch(AccessIntent.EditorRevoke)
        assertEquals(AccessEvent.ConfirmRevoke(phone), events.single())
        assertEquals(null, viewModel.state.value.editor)

        repository.mutable.value = repository.mutable.value.copy(tokens = listOf(owner))
        viewModel.dispatch(AccessIntent.Revoke(phone.id))
        assertEquals(listOf(owner), viewModel.state.value.tokens.tokens)
    }

    @Test
    fun refreshAndAServerFailureReachTheRepositoryAndTheState() = runTest(UnconfinedTestDispatcher()) {
        val repository = FakeRepository()
        val viewModel = AccessViewModel(repository)
        val events = mutableListOf<AccessEvent>()
        backgroundScope.launch { viewModel.events.toList(events) }

        viewModel.dispatch(AccessIntent.Refresh)
        assertEquals(1, repository.refreshes)

        repository.mutable.value = repository.mutable.value.copy(loading = true)
        repository.mutable.value = AccessTokensState(
            loading = false,
            tokens = listOf(phone),
            failure = RemoteFailure.ModuleForbidden("admin"),
        )
        assertEquals(RemoteFailure.ModuleForbidden("admin"), viewModel.state.value.tokens.failure)
        assertEquals(listOf(AccessEvent.Failure(RemoteFailure.ModuleForbidden("admin"))), events)
    }

    private companion object {
        val phone = AccessToken(
            id = "t-phone",
            name = "Телефон",
            modules = setOf(AppModule.BASE, AppModule.HOME),
            owner = false,
            createdUnix = 1_700_000_000L,
            updatedUnix = 1_700_000_100L,
        )

        val owner = AccessToken(
            id = "owner",
            name = "Owner",
            modules = AppModule.entries.toSet(),
            owner = true,
            createdUnix = 1_700_000_000L,
            updatedUnix = 1_700_000_100L,
        )

        val tablet = AccessToken(
            id = "t-new",
            name = "Планшет",
            modules = setOf(AppModule.BASE, AppModule.HOME, AppModule.NOTIFICATIONS),
            owner = false,
            createdUnix = 1_700_000_400L,
            updatedUnix = 1_700_000_400L,
        )
    }
}
