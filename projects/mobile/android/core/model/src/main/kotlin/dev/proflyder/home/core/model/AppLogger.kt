package dev.proflyder.home.core.model

enum class LogLevel { DEBUG, INFO, WARN, ERROR }

interface AppLogger {
    fun log(level: LogLevel, tag: String, message: String, error: Throwable? = null)
}

fun AppLogger.debug(tag: String, message: String) = log(LogLevel.DEBUG, tag, message)

fun AppLogger.info(tag: String, message: String) = log(LogLevel.INFO, tag, message)

fun AppLogger.warn(tag: String, message: String, error: Throwable? = null) = log(LogLevel.WARN, tag, message, error)

fun AppLogger.error(tag: String, message: String, error: Throwable? = null) = log(LogLevel.ERROR, tag, message, error)
