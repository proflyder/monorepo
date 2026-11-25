package dev.proflyder.home.core.designsystem.glass

import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.view.View
import androidx.core.graphics.createBitmap

internal class BlockSnapshots {
    private val bitmaps = mutableMapOf<View, Bitmap>()

    fun draw(canvas: Canvas, block: View, x: Float, y: Float) {
        if (block.width <= 0 || block.height <= 0) return
        val bitmap = bitmapFor(block)
        bitmap.eraseColor(Color.TRANSPARENT)
        block.draw(Canvas(bitmap))
        canvas.drawBitmap(bitmap, x, y, null)
    }

    fun release(block: View) {
        bitmaps.remove(block)?.recycle()
    }

    private fun bitmapFor(block: View): Bitmap {
        val cached = bitmaps[block]
        if (cached != null && cached.width == block.width && cached.height == block.height) return cached
        cached?.recycle()
        return createBitmap(block.width, block.height).also { bitmaps[block] = it }
    }
}
