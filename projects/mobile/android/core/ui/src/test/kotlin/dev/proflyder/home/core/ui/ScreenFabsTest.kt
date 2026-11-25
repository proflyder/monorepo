package dev.proflyder.home.core.ui

import android.view.ContextThemeWrapper
import android.view.Gravity
import android.view.View
import android.widget.FrameLayout
import android.widget.LinearLayout
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import com.google.android.material.floatingactionbutton.ExtendedFloatingActionButton
import com.google.android.material.floatingactionbutton.FloatingActionButton
import dev.proflyder.home.core.model.ConnectCode
import dev.proflyder.home.core.model.RemoteFailure
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config
import com.google.android.material.R as MaterialR

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "ru")
class ScreenFabsTest {
    private val context = ContextThemeWrapper(RuntimeEnvironment.getApplication(), MaterialR.style.Theme_Material3_DayNight)

    @Test
    fun bottomFabsClearTheChromeAndInlineFabsStay() {
        val root = FrameLayout(context)
        val screenFab = ExtendedFloatingActionButton(context)
        root.addView(screenFab, FrameLayout.LayoutParams(WRAP, WRAP, Gravity.BOTTOM or Gravity.END))
        val panel = LinearLayout(context)
        val inlineFab = FloatingActionButton(context)
        panel.addView(inlineFab, LinearLayout.LayoutParams(WRAP, WRAP))
        root.addView(panel, FrameLayout.LayoutParams(WRAP, WRAP, Gravity.BOTTOM))

        assertEquals(listOf<View>(screenFab), root.screenFabs().toList())
        root.liftScreenFabs()
        val insets = WindowInsetsCompat.Builder()
            .setInsets(WindowInsetsCompat.Type.navigationBars(), Insets.of(0, 0, 0, CHROME_BOTTOM))
            .build()
        ViewCompat.dispatchApplyWindowInsets(root, insets)

        assertEquals(CHROME_BOTTOM, (screenFab.layoutParams as FrameLayout.LayoutParams).bottomMargin)
        assertEquals(0, (inlineFab.layoutParams as LinearLayout.LayoutParams).bottomMargin)
    }

    @Test
    fun missingResourceIsLocalizedWithoutRawServerText() {
        val failure = RemoteFailure.Server(httpStatus = 404, code = ConnectCode.UNKNOWN, message = "")
        assertEquals("На сервере нет таких данных или функции (HTTP 404).", failure.userMessage(RuntimeEnvironment.getApplication()))
    }

    private companion object {
        const val WRAP = FrameLayout.LayoutParams.WRAP_CONTENT
        const val CHROME_BOTTOM = 210
    }
}
