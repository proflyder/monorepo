package dev.proflyder.home.core.designsystem

import android.graphics.drawable.Drawable
import com.google.android.material.button.MaterialButton
import com.google.android.material.progressindicator.CircularProgressIndicatorSpec
import com.google.android.material.progressindicator.IndeterminateDrawable

private class LoadingMemory(
    val text: CharSequence?,
    val icon: Drawable?,
    val iconGravity: Int,
    val iconPadding: Int,
    val minWidth: Int,
    val clickable: Boolean,
)

val MaterialButton.isLoading: Boolean
    get() = getTag(R.id.ds_button_loading_text) is LoadingMemory

fun MaterialButton.setLoading(loading: Boolean) {
    when {
        loading == isLoading -> Unit
        loading -> startLoading()
        else -> stopLoading()
    }
}

private fun MaterialButton.startLoading() {
    setTag(R.id.ds_button_loading_text, LoadingMemory(text, icon, iconGravity, iconPadding, minWidth, isClickable))
    minWidth = width
    icon = loadingDrawable()
    iconGravity = MaterialButton.ICON_GRAVITY_TEXT_TOP
    iconPadding = 0
    text = null
    isClickable = false
}

private fun MaterialButton.stopLoading() {
    val memory = getTag(R.id.ds_button_loading_text) as LoadingMemory
    setTag(R.id.ds_button_loading_text, null)
    icon = memory.icon
    iconGravity = memory.iconGravity
    iconPadding = memory.iconPadding
    text = memory.text
    minWidth = memory.minWidth
    isClickable = memory.clickable
}

private fun MaterialButton.loadingDrawable(): IndeterminateDrawable<CircularProgressIndicatorSpec> {
    val spec = CircularProgressIndicatorSpec(context, null, 0, LOADING_STYLE)
    spec.indicatorColors = intArrayOf(currentTextColor)
    return IndeterminateDrawable.createCircularDrawable(context, spec)
}

private val LOADING_STYLE = com.google.android.material.R.style.Widget_Material3_CircularProgressIndicator_ExtraSmall
