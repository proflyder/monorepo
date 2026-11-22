package dev.proflyder.home.core.network

import kotlinx.coroutines.flow.Flow

interface RouteGate {
    suspend fun await()
}

interface RouteCondition {
    val ready: Flow<Boolean>
}
