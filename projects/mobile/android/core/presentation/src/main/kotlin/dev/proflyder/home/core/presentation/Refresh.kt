package dev.proflyder.home.core.presentation

enum class RefreshMode {
    SOFT,
    HARD,
}

interface Refreshable {
    fun refresh(mode: RefreshMode)
}
