package dev.proflyder.home.core.designsystem

data class PopupPlacement(val top: Int, val height: Int, val above: Boolean)

object PopupPlacements {
    fun place(anchorTop: Int, anchorBottom: Int, limitTop: Int, limitBottom: Int, content: Int, gap: Int): PopupPlacement {
        val below = (limitBottom - anchorBottom - gap).coerceAtLeast(0)
        val above = (anchorTop - gap - limitTop).coerceAtLeast(0)
        return when {
            content <= below -> PopupPlacement(anchorBottom + gap, content, above = false)
            content <= above -> PopupPlacement(anchorTop - gap - content, content, above = true)
            above > below -> PopupPlacement(limitTop, above, above = true)
            else -> PopupPlacement(anchorBottom + gap, below, above = false)
        }
    }

    fun clampX(x: Int, width: Int, limitLeft: Int, limitRight: Int): Int =
        x.coerceAtMost(limitRight - width).coerceAtLeast(limitLeft)
}
