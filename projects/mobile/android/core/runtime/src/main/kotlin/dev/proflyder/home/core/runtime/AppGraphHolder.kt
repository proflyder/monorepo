package dev.proflyder.home.core.runtime

import android.content.Context

interface AppGraphHolder {
    val appGraph: Any
}

inline fun <reified T : Any> Context.appGraph(): T = (applicationContext as AppGraphHolder).appGraph as T
