package dev.proflyder.home.core.designsystem.glass

import android.graphics.Canvas
import android.graphics.RenderEffect
import android.graphics.RenderNode
import android.graphics.Shader
import androidx.annotation.RequiresApi

@RequiresApi(31)
internal class GlassBackdrop(radius: Float, private val background: Int) {
    private val node = RenderNode(NODE_NAME).apply {
        setRenderEffect(RenderEffect.createBlurEffect(radius, radius, Shader.TileMode.CLAMP))
    }

    fun draw(canvas: Canvas, width: Int, height: Int, source: GlassSource) {
        node.setPosition(0, 0, width, height)
        val recording = node.beginRecording(width, height)
        runCatching {
            recording.drawColor(background)
            source.drawBehind(recording)
        }.also { node.endRecording() }.getOrThrow()
        canvas.drawRenderNode(node)
    }

    private companion object {
        const val NODE_NAME = "home-glass"
    }
}
