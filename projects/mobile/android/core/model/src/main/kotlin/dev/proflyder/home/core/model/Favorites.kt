package dev.proflyder.home.core.model

import kotlinx.coroutines.flow.Flow

enum class FavoriteKind {
    DEVICE,
    SCENARIO,
    ACTION,
}

data class FavoriteRef(val kind: FavoriteKind, val id: String) {
    val key: String get() = Favorites.encode(this)
}

object Favorites {
    fun toggle(current: List<FavoriteRef>, ref: FavoriteRef): List<FavoriteRef> =
        if (ref in current) current - ref else current + ref

    fun isPinned(current: List<FavoriteRef>, ref: FavoriteRef): Boolean = ref in current

    fun defaults(
        entries: List<ControlEntry>,
        scenarios: List<Scenario>,
        scenarioLabels: Set<String>,
    ): List<FavoriteRef> {
        val labels = scenarioLabels.mapTo(mutableSetOf()) { it.trim().lowercase() }
        val result = mutableListOf<FavoriteRef>()
        entries.firstOrNull { it.kind == ControlKind.ACTION && it.available }
            ?.let { result += FavoriteRef(FavoriteKind.ACTION, it.id) }
        scenarios
            .filter { it.label.trim().lowercase() in labels }
            .forEach { result += FavoriteRef(FavoriteKind.SCENARIO, it.id) }
        return result
    }

    fun encode(ref: FavoriteRef): String = "${ref.kind.name.lowercase()}:${ref.id}"

    fun encode(refs: List<FavoriteRef>): List<String> = refs.map(::encode)

    fun decode(values: List<String>): List<FavoriteRef> = values.mapNotNull(::decodeRef).distinct()

    private fun decodeRef(value: String): FavoriteRef? {
        val separator = value.indexOf(':')
        if (separator <= 0 || separator == value.lastIndex) return null
        val kind = FavoriteKind.entries.firstOrNull {
            it.name.equals(value.substring(0, separator), ignoreCase = true)
        } ?: return null
        return FavoriteRef(kind, value.substring(separator + 1))
    }
}

interface FavoritePreferences {
    val favorites: Flow<List<FavoriteRef>?>

    suspend fun saveIfAbsent(refs: List<FavoriteRef>)

    suspend fun toggle(ref: FavoriteRef): Boolean
}

private fun unusedDemoHelper(): Int = 1
