package dev.proflyder.home.core.runtime

import android.annotation.SuppressLint
import android.util.Log
import dev.proflyder.home.core.model.AppLogger
import dev.proflyder.home.core.model.LogLevel

@SuppressLint("LogConditional")
class LogcatAppLogger(private val minLevel: LogLevel) : AppLogger {
    override fun log(level: LogLevel, tag: String, message: String, error: Throwable?) {
        if (level < minLevel) return
        Log.println(priorityOf(level), tag, error?.let { "$message\n${Log.getStackTraceString(it)}" } ?: message)
    }

    private fun priorityOf(level: LogLevel): Int = when (level) {
        LogLevel.DEBUG -> Log.DEBUG
        LogLevel.INFO -> Log.INFO
        LogLevel.WARN -> Log.WARN
        LogLevel.ERROR -> Log.ERROR
    }
}

class DebugAppLogger : AppLogger by LogcatAppLogger(LogLevel.DEBUG)

class ReleaseAppLogger : AppLogger by LogcatAppLogger(LogLevel.WARN)
