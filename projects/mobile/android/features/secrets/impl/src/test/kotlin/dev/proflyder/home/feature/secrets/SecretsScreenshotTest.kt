package dev.proflyder.home.feature.secrets

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.view.LayoutInflater
import android.view.View
import android.widget.FrameLayout
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.color.MaterialColors
import dev.proflyder.home.core.model.SecretVersion
import dev.proflyder.home.feature.secrets.impl.databinding.DialogSecretStepupBinding
import dev.proflyder.home.feature.secrets.impl.databinding.FragmentSecretPathBinding
import dev.proflyder.home.feature.secrets.impl.databinding.FragmentSecretsCatalogBinding
import dev.proflyder.home.feature.secrets.presentation.SecretPathAdapter
import dev.proflyder.home.feature.secrets.presentation.SecretPathRow
import dev.proflyder.home.feature.secrets.presentation.SecretsCatalogAdapter
import dev.proflyder.home.feature.secrets.presentation.SecretsCatalogGroup
import dev.proflyder.home.feature.secrets.presentation.catalogRows
import dev.proflyder.home.feature.secrets.presentation.keyRows
import dev.proflyder.home.feature.secrets.presentation.versionRows
import java.io.File
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
class SecretsScreenshotTest {
    private fun themed(): Context = ContextThemeWrapper(
        RuntimeEnvironment.getApplication(),
        com.google.android.material.R.style.Theme_Material3_DayNight_NoActionBar,
    )

    @Test
    fun theCatalogShowsGroupedPathsWithDescriptionsAndSearch() {
        val context = themed()
        val binding = FragmentSecretsCatalogBinding.inflate(LayoutInflater.from(context))
        val adapter = SecretsCatalogAdapter {}
        binding.secretsCatalogList.adapter = adapter
        adapter.submitList(
            catalogRows(
                listOf(
                    SecretsCatalogGroup(
                        "k8s",
                        listOf(
                            FakeSecretsRepository.entry("k8s/home-assistant/phone-push", "Push service token"),
                            FakeSecretsRepository.entry("k8s/test", "smoke probe, not secret"),
                        ),
                    ),
                ),
            ),
        )
        binding.secretsCatalogSearch.setText("k8s")
        binding.secretsCatalogProgress.visibility = View.GONE
        shadowOf(android.os.Looper.getMainLooper()).idle()

        val bitmap = render(surface(context, binding.root), 411, 891)
        save(bitmap, "secrets-catalog.png")
        assertTrue(bitmap.width > 0)
    }

    @Test
    fun thePathScreenShowsVersionsAndMasksKeyValues() {
        val context = themed()
        val binding = FragmentSecretPathBinding.inflate(LayoutInflater.from(context))
        val adapter = SecretPathAdapter(
            object : dev.proflyder.home.feature.secrets.presentation.SecretPathActions {
                override fun reveal(key: String) = Unit

                override fun copy(key: String) = Unit

                override fun edit(key: String) = Unit

                override fun rollback(version: Long) = Unit

                override fun delete() = Unit
            },
        )
        binding.secretPathList.adapter = adapter
        val rows = buildList {
            add(
                SecretPathRow.Header(
                    key = "header",
                    title = FakeSecretsRepository.PATH,
                    description = "Push service token",
                    currentVersion = 3,
                    sessionActive = true,
                ),
            )
            add(SecretPathRow.Note("keys:title", context.getString(dev.proflyder.home.feature.secrets.impl.R.string.secrets_keys)))
            addAll(keyRows(listOf("token", "endpoint"), revealedKey = null, revealedValue = null, version = 3, revealingKey = null))
            add(SecretPathRow.Note("versions:title", context.getString(dev.proflyder.home.feature.secrets.impl.R.string.secrets_versions)))
            addAll(
                versionRows(
                    versions = listOf(
                        SecretVersion(3, "2026-11-02T10:00:00Z", "", false),
                        SecretVersion(2, "2026-11-01T19:30:00Z", "", false),
                        SecretVersion(1, "2026-10-30T08:00:00Z", "2026-11-01T18:00:00Z", false),
                    ),
                    currentVersion = 3,
                    createdLabel = { if (it.isEmpty()) "" else "02 Nov 2026" },
                    rollbackLabel = context.getString(dev.proflyder.home.feature.secrets.impl.R.string.secrets_rollback),
                    unavailableLabel = {
                        if (it.available) null else context.getString(
                            dev.proflyder.home.feature.secrets.impl.R.string.secrets_version_deleted,
                        )
                    },
                ),
            )
        }
        adapter.submitList(rows)
        binding.secretPathProgress.visibility = View.GONE
        shadowOf(android.os.Looper.getMainLooper()).idle()

        val bitmap = render(surface(context, binding.root), 411, 891)
        save(bitmap, "secrets-path-masked.png")
        assertTrue(bitmap.width > 0)
    }

    @Test
    fun theTotpDialogAsksForSixDigits() {
        val context = themed()
        val binding = DialogSecretStepupBinding.inflate(LayoutInflater.from(context))
        binding.secretTotpInput.setText("123456")
        shadowOf(android.os.Looper.getMainLooper()).idle()
        val wrapper = FrameLayout(context).apply {
            setPadding(32, 32, 32, 32)
            setBackgroundColor(MaterialColors.getColor(this, com.google.android.material.R.attr.colorSurface))
            addView(binding.root)
        }
        val bitmap = render(wrapper, 411, 320)
        save(bitmap, "secrets-totp.png")
        assertTrue(bitmap.width > 0)
    }

    private fun surface(context: Context, view: View): View = FrameLayout(context).apply {
        setBackgroundColor(MaterialColors.getColor(this, com.google.android.material.R.attr.colorSurface))
        addView(view)
    }

    private fun render(view: View, widthDp: Int, heightDp: Int): Bitmap {
        val metrics = view.resources.displayMetrics
        val width = (widthDp * metrics.density).toInt()
        val height = (heightDp * metrics.density).toInt()
        repeat(2) {
            view.measure(exactly(width), exactly(height))
            view.layout(0, 0, width, height)
        }
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
}
