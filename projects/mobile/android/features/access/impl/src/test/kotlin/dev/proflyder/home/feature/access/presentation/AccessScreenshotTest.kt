package dev.proflyder.home.feature.access.presentation

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import android.widget.FrameLayout
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.model.AccessToken
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.feature.access.impl.databinding.DialogAccessCreatedBinding
import dev.proflyder.home.feature.access.impl.databinding.FragmentAccessBinding
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35], qualifiers = "ru-w411dp-h891dp-xxhdpi")
@GraphicsMode(GraphicsMode.Mode.NATIVE)
class AccessScreenshotTest {
    private fun themed(): Context = ContextThemeWrapper(
        RuntimeEnvironment.getApplication(),
        com.google.android.material.R.style.Theme_Material3_DayNight_NoActionBar,
    )

    @Test
    fun theAccessScreenListsOwnerAndNamedTokensWithTheirModules() {
        val context = themed()
        val binding = FragmentAccessBinding.inflate(LayoutInflater.from(context))
        val adapter = AccessAdapter(AccessAdapterCallback({}, { _, _ -> }))
        binding.accessList.adapter = adapter
        adapter.submitList(tokens)
        shadowOf(android.os.Looper.getMainLooper()).idle()
        binding.accessProgress.visibility = View.GONE
        binding.accessError.visibility = View.GONE
        binding.accessEmpty.visibility = View.GONE

        val wrapper = FrameLayout(context).apply {
            setBackgroundColor(MaterialColors.getColor(this, com.google.android.material.R.attr.colorSurface))
            addView(binding.root)
        }
        val bitmap = render(wrapper, 411, 891)
        save(bitmap, "access-tokens.png")
        assertEquals(2, binding.accessList.adapter?.itemCount)
        assertTrue(bitmap.width > 0)
    }

    @Test
    fun theOneTimeTokenDialogShowsThePlainTokenAndACopyAction() {
        val context = themed()
        val binding = DialogAccessCreatedBinding.inflate(LayoutInflater.from(context))
        binding.accessCreatedToken.text = ONE_TIME_TOKEN
        binding.accessCreatedToken.contentDescription = context.getString(
            dev.proflyder.home.feature.access.impl.R.string.access_token_description,
            "Планшет",
        )
        binding.accessCreatedCopy.text = context.getString(
            dev.proflyder.home.feature.access.impl.R.string.access_created_copy,
        )
        val wrapper = FrameLayout(context).apply {
            setPadding(32, 32, 32, 32)
            setBackgroundColor(MaterialColors.getColor(this, com.google.android.material.R.attr.colorSurface))
            addView(binding.root)
        }

        val bitmap = render(wrapper, 411, 300)
        save(bitmap, "access-token-created.png")
        assertEquals(ONE_TIME_TOKEN, binding.accessCreatedToken.text.toString())
        assertEquals(View.VISIBLE, binding.accessCreatedCopy.visibility)
    }

    private fun render(view: View, widthDp: Int, heightDp: Int): Bitmap {
        val metrics = view.resources.displayMetrics
        val width = (widthDp * metrics.density).toInt()
        val height = (heightDp * metrics.density).toInt()
        view.measure(exactly(width), exactly(height))
        view.layout(0, 0, width, height)
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        view.draw(Canvas(bitmap))
        return bitmap
    }

    private fun save(bitmap: Bitmap, name: String) {
        val dir = System.getenv("TEST_UNDECLARED_OUTPUTS_DIR") ?: return
        File(dir).mkdirs()
        File(dir, name).outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
    }

    private fun exactly(size: Int): Int = View.MeasureSpec.makeMeasureSpec(size, View.MeasureSpec.EXACTLY)

    private companion object {
        const val ONE_TIME_TOKEN = "home_0123456789abcdef0123456789abcdef"

        val tokens = listOf(
            AccessToken(
                id = "owner",
                name = "Owner",
                modules = AppModule.entries.toSet(),
                owner = true,
                createdUnix = 1_700_000_000L,
                updatedUnix = 1_700_000_000L,
            ),
            AccessToken(
                id = "t-phone",
                name = "Телефон кухни",
                modules = setOf(AppModule.BASE, AppModule.HOME, AppModule.ALARMS_TIMERS, AppModule.NOTIFICATIONS),
                owner = false,
                createdUnix = 1_700_000_100L,
                updatedUnix = 1_700_000_200L,
            ),
        )
    }
}
