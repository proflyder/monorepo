package dev.proflyder.home.feature.control

/**
 * Rules for the "speak on the station" text, shared by the dialog and the
 * tests. The server re-validates everything: this is only the local guard that
 * keeps the "Say" button disabled for input the executor would reject.
 */
object SpeakText {
    /** Same limit as the server (`speak` action) and the HA script field. */
    const val MAX_LENGTH = 500

    /**
     * The text as the server should receive it, or null when it is not
     * speakable. Line breaks and repeated spaces collapse to one space,
     * because the executor rejects control characters.
     */
    fun normalize(raw: String): String? {
        val text = raw.trim().split(WHITESPACE).filter { it.isNotEmpty() }.joinToString(" ")
        if (text.isEmpty() || text.length > MAX_LENGTH) return null
        if (text.any { it.isISOControl() }) return null
        return text
    }

    /** Characters left for the counter, never negative. */
    fun remaining(raw: String): Int = (MAX_LENGTH - raw.trim().length).coerceAtLeast(0)

    private val WHITESPACE = Regex("\\s+")
}
