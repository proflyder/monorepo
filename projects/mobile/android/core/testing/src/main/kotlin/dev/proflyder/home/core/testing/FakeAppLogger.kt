package dev.proflyder.home.core.testing

import dev.proflyder.home.core.model.AppLogger
import dev.proflyder.home.core.model.LogLevel

class FakeAppLogger : AppLogger {
    data class Entry(val level: LogLevel, val tag: String, val message: String, val error: Throwable?)

    val entries = mutableListOf<Entry>()

    override fun log(level: LogLevel, tag: String, message: String, error: Throwable?) {
        entries += Entry(level, tag, message, error)
    }
}
