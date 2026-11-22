package dev.proflyder.home.core.designsystem

import android.graphics.Bitmap
import android.graphics.Canvas
import android.os.Bundle
import android.view.View
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.NumberPicker
import androidx.fragment.app.FragmentActivity
import dev.proflyder.home.core.theme.AppTheme
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import android.os.Looper

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "ru-w411dp-h891dp-night-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class SheetsScreenshotTest {
    @Test
    fun optionsSheetMarksTheSelectionAndReturnsTheTappedKey() {
        val activity = activity()
        val results = mutableListOf<Bundle>()
        activity.supportFragmentManager.setFragmentResultListener(REQUEST, activity) { _, result -> results += result }
        OptionsSheet.show(activity.supportFragmentManager, REQUEST, "Добавить шаг", OPTIONS, selectedKey = "speak")
        val sheet = settle(activity) as OptionsSheet
        save(sheet.requireView(), "options_sheet.png")

        sheet.requireView().findViewById<LinearLayout>(R.id.ds_sheet_options).getChildAt(2).performClick()
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals("delay", OptionsSheet.chosenKey(results.single()))
    }

    @Test
    fun cancellingTheOptionsSheetReportsNoChoice() {
        val activity = activity()
        val results = mutableListOf<Bundle>()
        activity.supportFragmentManager.setFragmentResultListener(REQUEST, activity) { _, result -> results += result }
        OptionsSheet.show(activity.supportFragmentManager, REQUEST, "Добавить шаг", OPTIONS)
        (settle(activity) as OptionsSheet).requireDialog().cancel()
        shadowOf(Looper.getMainLooper()).idle()

        assertNull(OptionsSheet.chosenKey(results.single()))
    }

    @Test
    fun wheelSheetConfirmsTheScrolledValue() {
        val activity = activity()
        val results = mutableListOf<Bundle>()
        activity.supportFragmentManager.setFragmentResultListener(REQUEST, activity) { _, result -> results += result }
        WheelSheet.show(activity.supportFragmentManager, REQUEST, "Задержка", WHEEL, selected = 2)
        val sheet = settle(activity) as WheelSheet
        save(sheet.requireView(), "wheel_sheet.png")
        val wheel = sheet.requireView().findViewById<NumberPicker>(R.id.ds_sheet_wheel)
        assertEquals(2, wheel.value)

        wheel.value = 4
        wheel.dispatchValueChange(4)
        sheet.requireView().findViewById<View>(R.id.ds_sheet_done).performClick()
        shadowOf(Looper.getMainLooper()).idle()

        assertEquals(4, WheelSheet.chosenIndex(results.single()))
    }

    private fun NumberPicker.dispatchValueChange(index: Int) {
        val listener = NumberPicker::class.java.getDeclaredField("mOnValueChangeListener").apply { isAccessible = true }
        (listener.get(this) as NumberPicker.OnValueChangeListener).onValueChange(this, index, index)
    }

    private fun activity(): FragmentActivity {
        val controller = Robolectric.buildActivity(FragmentActivity::class.java)
        controller.get().setTheme(R.style.Base_Theme_Home)
        controller.get().theme.applyStyle(AppTheme.entries.first().overlayRes, true)
        return controller.setup().get()
    }

    private fun settle(activity: FragmentActivity) = activity.supportFragmentManager.run {
        executePendingTransactions()
        shadowOf(Looper.getMainLooper()).idle()
        checkNotNull(findFragmentByTag(REQUEST))
    }

    private fun save(view: View, name: String) {
        (view.parent as? android.view.ViewGroup)?.removeView(view)
        val host = FrameLayout(view.context).apply { addView(view) }
        host.setBackgroundColor(color(view.context, com.google.android.material.R.attr.colorSurfaceContainerLow))
        val width = View.MeasureSpec.makeMeasureSpec(WIDTH, View.MeasureSpec.EXACTLY)
        host.measure(width, View.MeasureSpec.makeMeasureSpec(0, View.MeasureSpec.UNSPECIFIED))
        host.layout(0, 0, host.measuredWidth, host.measuredHeight)
        val bitmap = Bitmap.createBitmap(host.measuredWidth, host.measuredHeight, Bitmap.Config.ARGB_8888)
        host.draw(Canvas(bitmap))
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
    }

    private companion object {
        const val REQUEST = "sheet_request"
        const val WIDTH = 1233
        const val PNG_QUALITY = 100
        val OPTIONS = listOf(
            SheetOption("action", "Действие устройства", R.drawable.ds_ic_add),
            SheetOption("speak", "Сказать фразу", R.drawable.ds_ic_open_in_new),
            SheetOption("delay", "Пауза"),
        )
        val WHEEL = listOf("5 с", "10 с", "30 с", "1 мин", "5 мин", "10 мин")
    }
}
