package dev.proflyder.home.feature.secrets

import android.os.Looper
import android.view.WindowManager
import dev.proflyder.home.core.navigation.RouteArgs
import dev.proflyder.home.feature.secrets.presentation.SecretPathFragment
import dev.proflyder.home.feature.secrets.presentation.SecretPathViewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

@OptIn(ExperimentalCoroutinesApi::class)
@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class SecretsScreenSecureTest {
    @Before
    fun setUp() {
        Dispatchers.setMain(UnconfinedTestDispatcher())
        val application = RuntimeEnvironment.getApplication() as SecretsTestApplication
        application.viewModel = SecretPathViewModel(FakeSecretsRepository())
    }

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun secretScreensMarkTheWindowSecureAndClearItOnLeave() {
        val activity = Robolectric.buildActivity(SecretsTestActivity::class.java).setup().get()
        val fragment = SecretPathFragment().apply {
            arguments = RouteArgs.bundle(SecretsPathDestination(FakeSecretsRepository.PATH))
        }

        activity.supportFragmentManager.beginTransaction()
            .add(android.R.id.content, fragment)
            .commitNow()
        shadowOf(Looper.getMainLooper()).idle()

        assertTrue(activity.secure)

        activity.supportFragmentManager.beginTransaction().remove(fragment).commitNow()
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(false, activity.secure)
    }

    private val SecretsTestActivity.secure: Boolean
        get() = window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE != 0
}
