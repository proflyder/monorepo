package dev.proflyder.home.core.presentation

import androidx.activity.ComponentDialog
import androidx.activity.OnBackPressedCallback
import androidx.activity.OnBackPressedDispatcher
import androidx.fragment.app.DialogFragment
import androidx.fragment.app.Fragment

val Fragment.backDispatcher: OnBackPressedDispatcher
    get() = generateSequence(this, Fragment::getParentFragment)
        .firstNotNullOfOrNull { (it as? DialogFragment)?.dialog as? ComponentDialog }
        ?.onBackPressedDispatcher
        ?: requireActivity().onBackPressedDispatcher

fun Fragment.addBackCallback(callback: OnBackPressedCallback) {
    backDispatcher.addCallback(viewLifecycleOwner, callback)
}
