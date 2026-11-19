package dev.proflyder.home.feature.settings.presentation

import android.content.res.ColorStateList
import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import androidx.core.view.ViewCompat
import androidx.fragment.app.viewModels
import com.google.android.material.button.MaterialButtonToggleGroup
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.theme.AppThemes
import dev.proflyder.home.core.theme.Appearance
import dev.proflyder.home.core.theme.ColorRole
import dev.proflyder.home.core.theme.R as ThemeR
import dev.proflyder.home.core.theme.ThemeMode
import dev.proflyder.home.core.ui.BaseFragment
import dev.proflyder.home.core.ui.viewModelFactory
import dev.proflyder.home.feature.settings.impl.R
import dev.proflyder.home.feature.settings.impl.databinding.FragmentAppearanceBinding
import dev.proflyder.home.feature.settings.impl.databinding.ItemAppearanceThemeBinding

/**
 * Light/dark mode plus one preview card per registry entry. Each card is
 * inflated in its own theme, so it shows that palette's real surface, text and
 * swatches; a new [AppTheme] entry appears here without touching this screen.
 *
 * A theme change recreates the activity (the overlay is applied before the
 * first frame); a mode change is applied by AppCompat itself.
 */
class AppearanceFragment : BaseFragment<FragmentAppearanceBinding>(FragmentAppearanceBinding.Companion::inflate) {
    private val viewModel: AppearanceViewModel by viewModels {
        viewModelFactory(AppearanceViewModel::class.java)
    }

    private val cards = linkedMapOf<AppTheme, ItemAppearanceThemeBinding>()

    override fun onBindingCreated(binding: FragmentAppearanceBinding, savedInstanceState: Bundle?) {
        bindThemeCards(binding, AppThemes.isDynamicAvailable) { onThemeClicked(it) }
        bindModeButtons(binding.appearanceMode) { viewModel.selectMode(it) }
        viewModel.state.observe { render(it) }
    }

    override fun onBindingDestroyed() {
        cards.clear()
    }

    internal fun onThemeClicked(theme: AppTheme) {
        if (viewModel.selectTheme(theme)) requireActivity().recreate()
    }

    private fun render(appearance: Appearance) {
        renderAppearance(binding, cards, appearance)
    }

    internal fun bindThemeCards(binding: FragmentAppearanceBinding, dynamic: Boolean, onClick: (AppTheme) -> Unit) {
        AppTheme.entries.forEach { theme ->
            cards[theme] = addThemeCard(binding, theme, dynamic).also { it.root.setOnClickListener { onClick(theme) } }
        }
    }
}

private val MODE_BUTTONS = mapOf(
    ThemeMode.SYSTEM to R.id.appearance_mode_system,
    ThemeMode.LIGHT to R.id.appearance_mode_light,
    ThemeMode.DARK to R.id.appearance_mode_dark,
)

private val SWATCH_ROLES = listOf(
    ColorRole.PRIMARY,
    ColorRole.SECONDARY,
    ColorRole.TERTIARY,
    ColorRole.PRIMARY_CONTAINER,
    ColorRole.SURFACE,
)

/** One preview card, inflated in the theme it previews. */
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

internal fun bindModeButtons(group: MaterialButtonToggleGroup, onSelect: (ThemeMode) -> Unit) {
    group.addOnButtonCheckedListener { _, id, checked ->
        MODE_BUTTONS.entries.firstOrNull { it.value == id && checked }?.let { onSelect(it.key) }
    }
}

internal fun renderAppearance(
    binding: FragmentAppearanceBinding,
    cards: Map<AppTheme, ItemAppearanceThemeBinding>,
    appearance: Appearance,
) {
    MODE_BUTTONS[appearance.mode]?.let { binding.appearanceMode.check(it) }
    cards.forEach { (theme, card) -> renderCard(card, theme == appearance.theme) }
}

private fun renderCard(card: ItemAppearanceThemeBinding, selected: Boolean) {
    card.appearanceThemeRadio.isChecked = selected
    card.appearanceThemeRadio.jumpDrawablesToCurrentState()
    card.root.strokeWidth = when {
        selected -> card.root.resources.getDimensionPixelSize(R.dimen.settings_appearance_selected_stroke)
        else -> 0
    }
}
