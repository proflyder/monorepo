package dev.proflyder.home.feature.push

/**
 * Truncates to the server's byte bound without splitting a UTF-8 character.
 *
 * The scheduler stores `deviceName` in 64 bytes, while a Kotlin `take` counts
 * characters: a Cyrillic name would otherwise be rejected after a few words.
 */
internal fun String.takeUtf8Bytes(maxBytes: Int): String {
    if (toByteArray(Charsets.UTF_8).size <= maxBytes) return this
    var end = length
    while (end > 0 && substring(0, end).toByteArray(Charsets.UTF_8).size > maxBytes) {
        end -= 1
    }
    return substring(0, end)
}
