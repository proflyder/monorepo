package dev.proflyder.home.core.text

object SpeakText {
    const val MAX_LENGTH = 500

    fun normalize(raw: String): String? {
        val text = raw.trim().split(WHITESPACE).filter { it.isNotEmpty() }.joinToString(" ")
        if (text.isEmpty() || text.length > MAX_LENGTH) return null
        if (text.any { it.isISOControl() }) return null
        return text
    }

    fun remaining(raw: String): Int = (MAX_LENGTH - raw.trim().length).coerceAtLeast(0)

    private val WHITESPACE = Regex("\\s+")
}
