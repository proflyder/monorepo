package dev.proflyder.home.core.designsystem.glass

import android.view.View
import androidx.core.graphics.drawable.toDrawable
import android.graphics.Color
import android.content.Context
import android.graphics.Canvas
import android.os.Build
import androidx.annotation.RequiresApi

fun interface GlassSource {
    fun drawBehind(canvas: Canvas)
}

class GlassSurface(context: Context) {
    val style = GlassStyle(context)
    private var backdrop: Any? = null

    fun draw(canvas: Canvas, width: Int, height: Int, source: GlassSource) {
        if (width <= 0 || height <= 0) return
        val blurred = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && canvas.isHardwareAccelerated
        if (blurred) backdrop().draw(canvas, width, height, source)
        style.drawTint(canvas, width, 0, height, blurred)
    }

    @RequiresApi(Build.VERSION_CODES.S)
    private fun backdrop(): GlassBackdrop =
        backdrop as? GlassBackdrop ?: GlassBackdrop(style.blurRadius, style.backdropColor).also { backdrop = it }
}

internal fun View.receiveBorderlessRipples() {
    if (background == null) background = Color.TRANSPARENT.toDrawable()
}
