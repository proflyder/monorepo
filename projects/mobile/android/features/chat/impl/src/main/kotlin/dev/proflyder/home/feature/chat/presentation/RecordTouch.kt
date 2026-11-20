package dev.proflyder.home.feature.chat.presentation

import android.annotation.SuppressLint
import android.view.MotionEvent
import android.view.View

internal interface RecordGesture {
    fun onRecordStart(): Boolean

    fun onRecordSend()

    fun onRecordCancel()
}

internal class RecordTouch(
    private val gesture: RecordGesture,
    private val cancelDistancePx: Float,
) : View.OnTouchListener {
    private var downX = 0f
    private var active = false

    @SuppressLint("ClickableViewAccessibility")
    override fun onTouch(view: View, event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> down(view, event)
            MotionEvent.ACTION_MOVE -> move(event)
            MotionEvent.ACTION_UP -> finish(send = true)
            MotionEvent.ACTION_CANCEL -> finish(send = false)
            else -> Unit
        }
        return true
    }

    private fun down(view: View, event: MotionEvent) {
        downX = event.rawX
        view.isPressed = true
        active = gesture.onRecordStart()
    }

    private fun move(event: MotionEvent) {
        if (active && downX - event.rawX > cancelDistancePx) finish(send = false)
    }

    private fun finish(send: Boolean) {
        if (!active) return
        active = false
        if (send) gesture.onRecordSend() else gesture.onRecordCancel()
    }
}
