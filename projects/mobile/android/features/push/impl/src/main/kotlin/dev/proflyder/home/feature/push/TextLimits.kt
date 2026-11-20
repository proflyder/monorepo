package dev.proflyder.home.feature.push

internal fun String.takeUtf8Bytes(maxBytes: Int): String {
    if (toByteArray(Charsets.UTF_8).size <= maxBytes) return this
    var end = length
    while (end > 0 && substring(0, end).toByteArray(Charsets.UTF_8).size > maxBytes) {
        end -= 1
    }
    return substring(0, end)
}
