package dev.proflyder.home.feature.access

import android.content.ClipboardManager
import android.content.DialogInterface
import android.os.Looper
import androidx.appcompat.app.AlertDialog
import androidx.fragment.app.FragmentActivity
import dev.proflyder.home.core.model.AccessToken
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.model.CreatedAccessToken
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.feature.access.impl.R
import dev.proflyder.home.feature.access.presentation.AccessFragment
import dev.proflyder.home.feature.access.presentation.AccessIntent
import dev.proflyder.home.feature.access.presentation.AccessViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.shadows.ShadowDialog

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class AccessFragmentTest {
    private class FakeRepository : AccessRepository {
        private val mutable = MutableStateFlow(AccessTokensState(loading = false, tokens = listOf(phone)))
        var revoked: String? = null

        override val state: StateFlow<AccessTokensState> = mutable

        override fun refresh() = Unit

        override suspend fun create(name: String, modules: Set<AppModule>): Outcome<CreatedAccessToken> =
            Outcome.Success(CreatedAccessToken(ONE_TIME_TOKEN, tablet))

        override suspend fun update(id: String, name: String, modules: Set<AppModule>): Outcome<AccessToken> =
            Outcome.Success(tablet)

        override suspend fun revoke(id: String): Outcome<Unit> {
            revoked = id
            mutable.value = mutable.value.copy(tokens = mutable.value.tokens.filterNot { it.id == id })
            return Outcome.Success(Unit)
        }
    }

    private lateinit var repository: FakeRepository
    private lateinit var viewModel: AccessViewModel

    @Before
    fun setUp() {
        val app = RuntimeEnvironment.getApplication() as AccessTestApplication
        repository = FakeRepository()
        viewModel = AccessViewModel(repository)
        app.viewModel = viewModel
    }

    private fun show(): Pair<FragmentActivity, AccessFragment> {
        val activity = Robolectric.buildActivity(AccessTestActivity::class.java).setup().get()
        val fragment = AccessFragment()
        activity.supportFragmentManager.beginTransaction().add(android.R.id.content, fragment).commitNow()
        idle()
        return activity to fragment
    }

    private fun idle() = shadowOf(Looper.getMainLooper()).idle()

    private fun dialog(): AlertDialog = ShadowDialog.getLatestDialog() as AlertDialog

    @Test
    fun theEditorDialogIsReusedForTypingAndDismissedWhenTheStateCloses() {
        show()
        viewModel.dispatch(AccessIntent.New)
        idle()
        val editor = dialog()
        assertTrue(editor.isShowing)
        assertNotNull(editor.findViewById<android.view.View>(R.id.access_editor_name))

        viewModel.dispatch(AccessIntent.EditorName("Планшет"))
        idle()
        assertSame(editor, dialog())
        val name = checkNotNull(editor.findViewById<android.widget.EditText>(R.id.access_editor_name))
        assertEquals("Планшет", name.text.toString())

        viewModel.dispatch(AccessIntent.EditorDismiss)
        idle()
        assertFalse(editor.isShowing)
    }

    @Test
    fun theSameTokenReopensAfterTheEditorIsClosedWithBack() {
        show()
        viewModel.dispatch(AccessIntent.Edit(phone))
        idle()
        val first = dialog()
        first.cancel()
        idle()
        assertFalse(first.isShowing)
        assertEquals(null, viewModel.state.value.editor)

        viewModel.dispatch(AccessIntent.Edit(phone))
        idle()
        assertTrue(dialog().isShowing)
        assertNotSame(first, dialog())
    }

    @Test
    fun theSameTokenReopensAfterTheEditorIsClosedWithCancel() {
        show()
        viewModel.dispatch(AccessIntent.Edit(phone))
        idle()
        val first = dialog()
        first.getButton(DialogInterface.BUTTON_NEGATIVE).performClick()
        idle()
        assertFalse(first.isShowing)
        assertEquals(null, viewModel.state.value.editor)

        viewModel.dispatch(AccessIntent.Edit(phone))
        idle()
        assertTrue(dialog().isShowing)
    }

    @Test
    fun savingClosesTheEditorWithoutReopeningIt() {
        show()
        viewModel.dispatch(AccessIntent.Edit(phone))
        idle()
        val editor = dialog()
        editor.getButton(DialogInterface.BUTTON_POSITIVE).performClick()
        idle()
        assertFalse(editor.isShowing)
        assertEquals(null, viewModel.state.value.editor)
        assertSame(editor, dialog())
    }

    @Test
    fun aCreatedTokenIsShownOnceAndCopiedFromTheDialog() {
        show()
        viewModel.dispatch(AccessIntent.New)
        viewModel.dispatch(AccessIntent.EditorName("Планшет"))
        viewModel.dispatch(AccessIntent.EditorSave)
        idle()

        val created = dialog()
        assertTrue(created.isShowing)
        val token = checkNotNull(created.findViewById<android.widget.TextView>(R.id.access_created_token))
        assertEquals(ONE_TIME_TOKEN, token.text.toString())
        checkNotNull(created.findViewById<android.view.View>(R.id.access_created_copy)).performClick()
        idle()

        val clipboard = RuntimeEnvironment.getApplication().getSystemService(ClipboardManager::class.java)
        assertEquals(ONE_TIME_TOKEN, clipboard.primaryClip?.getItemAt(0)?.text.toString())

        viewModel.dispatch(AccessIntent.CreatedDismiss)
        idle()
        assertFalse(created.isShowing)
    }

    @Test
    fun revokingATokenAsksForConfirmationBeforeItReachesTheRepository() {
        show()
        repository.revoked = null
        viewModel.dispatch(AccessIntent.RevokeRequest(phone))
        idle()

        val confirm = dialog()
        assertEquals(null, repository.revoked)
        confirm.getButton(DialogInterface.BUTTON_POSITIVE).performClick()
        idle()

        assertEquals(phone.id, repository.revoked)
        assertFalse(confirm.isShowing)
    }

    private companion object {
        const val ONE_TIME_TOKEN = "home_0123456789abcdef0123456789abcdef"

        val phone = AccessToken(
            id = "t-phone",
            name = "Телефон",
            modules = setOf(AppModule.BASE, AppModule.HOME),
            owner = false,
            createdUnix = 1_700_000_000L,
            updatedUnix = 1_700_000_000L,
        )

        val tablet = AccessToken(
            id = "t-new",
            name = "Планшет",
            modules = setOf(AppModule.BASE, AppModule.HOME),
            owner = false,
            createdUnix = 1_700_000_100L,
            updatedUnix = 1_700_000_100L,
        )
    }
}

class AccessTestActivity : FragmentActivity()
