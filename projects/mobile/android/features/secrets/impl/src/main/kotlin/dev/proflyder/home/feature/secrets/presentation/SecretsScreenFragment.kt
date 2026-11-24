package dev.proflyder.home.feature.secrets.presentation

import android.os.Bundle
import android.view.WindowManager
import dev.proflyder.home.core.presentation.ScreenFragment
import dev.proflyder.home.core.ui.ViewBinding

internal abstract class SecretsScreenFragment<VB : ViewBinding, S : Any, I : Any, E : Any>(
    inflateBinding: (android.view.LayoutInflater, android.view.ViewGroup?, Boolean) -> VB,
) : ScreenFragment<VB, S, I, E>(inflateBinding) {
    override fun onScreenCreate(savedInstanceState: Bundle?) {
        super.onScreenCreate(savedInstanceState)
        requireActivity().window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
    }

    override fun onDestroy() {
        super.onDestroy()
        if (!anotherSecretsScreen()) {
            requireActivity().window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
        }
    }

    private fun anotherSecretsScreen(): Boolean =
        parentFragmentManager.fragments.any { fragment ->
            fragment !== this && fragment is SecretsScreenFragment<*, *, *, *> && fragment.isAdded
        }
}
