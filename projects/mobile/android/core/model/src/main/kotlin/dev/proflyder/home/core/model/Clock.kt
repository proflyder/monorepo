package dev.proflyder.home.core.model

interface Clock {
    fun wallMillis(): Long

    fun elapsedNanos(): Long
}
