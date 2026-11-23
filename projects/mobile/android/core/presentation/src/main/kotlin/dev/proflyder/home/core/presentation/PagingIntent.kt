package dev.proflyder.home.core.presentation

import dev.proflyder.home.core.data.RefreshMode

sealed interface PagingIntent {
    data class Refresh(val mode: RefreshMode = RefreshMode.SOFT) : PagingIntent

    data object LoadMore : PagingIntent

    data object Retry : PagingIntent
}

sealed interface ListItemIntent {
    data class Remove(val key: String) : ListItemIntent

    data object Undo : ListItemIntent
}
