package dev.proflyder.home.feature.settings.presentation

import dev.proflyder.home.core.presentation.bindPills
import android.content.res.ColorStateList
import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import androidx.core.view.ViewCompat
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.theme.AppThemes
import dev.proflyder.home.core.theme.Appearance
import dev.proflyder.home.core.theme.ColorRole
import dev.proflyder.home.core.theme.R as ThemeR
import dev.proflyder.home.core.theme.ThemeMode
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.presentation.screenViewModel
import dev.proflyder.home.feature.settings.impl.R
import dev.proflyder.home.feature.settings.impl.databinding.FragmentAppearanceBinding
import dev.proflyder.home.feature.settings.impl.databinding.ItemAppearanceThemeBinding

internal class AppearanceFragment :
    ScreenFragment<FragmentAppearanceBinding, Appearance, AppearanceIntent, Nothing>(
        FragmentAppearanceBinding.Companion::inflate,
    ) {
    override val viewModel: AppearanceViewModel by screenViewModel()

    override fun onViewReady(binding: FragmentAppearanceBinding, savedInstanceState: Bundle?) {
        val cards = bindThemeCards(binding, AppThemes.isDynamicAvailable) { dispatch(AppearanceIntent.SelectTheme(it)) }
        binding.appearanceMode.setLabels(ThemeMode.entries.map { getString(MODE_LABELS.getValue(it)) })
        val mode = bindPills(binding.appearanceMode, ThemeMode.entries, AppearanceIntent::SelectMode)
        render(Appearance::mode, mode::render)
        render(Appearance::theme) { theme -> renderThemeCards(cards, theme) }
    }
}

internal fun bindThemeCards(
    binding: FragmentAppearanceBinding,
    dynamic: Boolean,
    onClick: (AppTheme) -> Unit,
): Map<AppTheme, ItemAppearanceThemeBinding> = AppTheme.entries.associateWith { theme ->
    addThemeCard(binding, theme, dynamic).also { it.root.setOnClickListener { onClick(theme) } }
}

private val MODE_LABELS = mapOf(
    ThemeMode.SYSTEM to R.string.settings_appearance_mode_system,
    ThemeMode.LIGHT to R.string.settings_appearance_mode_light,
    ThemeMode.DARK to R.string.settings_appearance_mode_dark,
)

private val SWATCH_ROLES = listOf(
    ColorRole.PRIMARY,
    ColorRole.SECONDARY,
    ColorRole.TERTIARY,
    ColorRole.PRIMARY_CONTAINER,
    ColorRole.SURFACE,
)

internal fun addThemeCard(
    binding: FragmentAppearanceBinding,
    theme: AppTheme,
    dynamic: Boolean,
): ItemAppearanceThemeBinding {
    val context = AppThemes.wrap(binding.root.context, theme)
    val card = ItemAppearanceThemeBinding.inflate(LayoutInflater.from(context), binding.appearanceThemes, true)
    card.appearanceThemeTitle.setText(theme.titleRes)
    card.appearanceThemeSummary.setText(themeSummary(theme, dynamic))
    val swatches = listOf(
        card.appearanceSwatch1,
        card.appearanceSwatch2,
        card.appearanceSwatch3,
        card.appearanceSwatch4,
        card.appearanceSwatch5,
    )
    swatches.zip(SWATCH_ROLES).forEach { (view, role) -> tint(view, AppThemes.color(context, theme, role)) }
    return card
}

private fun themeSummary(theme: AppTheme, dynamic: Boolean): Int = when {
    theme.dynamic && !dynamic -> ThemeR.string.theme_system_unavailable
    else -> theme.summaryRes
}

private fun tint(view: View, color: Int) {
    ViewCompat.setBackgroundTintList(view, ColorStateList.valueOf(color))
}

internal fun renderThemeCards(cards: Map<AppTheme, ItemAppearanceThemeBinding>, selected: AppTheme) {
    cards.forEach { (theme, card) -> renderCard(card, theme == selected) }
}

private fun renderCard(card: ItemAppearanceThemeBinding, selected: Boolean) {
    card.appearanceThemeRadio.isChecked = selected
    card.appearanceThemeRadio.jumpDrawablesToCurrentState()
    card.root.strokeWidth = when {
        selected -> card.root.resources.getDimensionPixelSize(R.dimen.settings_appearance_selected_stroke)
        else -> 0
    }
}
