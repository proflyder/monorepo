package dev.proflyder.home.core.designsystem.glass

import android.graphics.Canvas
import android.graphics.RenderEffect
import android.graphics.RenderNode
import android.graphics.Shader
import android.view.View
import androidx.annotation.RequiresApi

@RequiresApi(31)
internal class BlurBackdrop(radius: Float) {
    private val node = RenderNode("home-glass-bar")
    private val effect = RenderEffect.createBlurEffect(radius, radius, Shader.TileMode.CLAMP)

    init {
        node.setRenderEffect(effect)
    }

    fun draw(canvas: Canvas, content: View, width: Int, height: Int, dirty: Boolean) {
        if (dirty) record(content, width, height)
        canvas.drawRenderNode(node)
    }

    private fun record(content: View, width: Int, height: Int) {
        val recording = node.beginRecording(width, height)
        recording.clipRect(0, 0, width, height)
        content.draw(recording)
        node.endRecording()
    }
}
