package dev.proflyder.home.core.model

/** What a pinned Home shortcut points at. */
enum class FavoriteKind {
    DEVICE,
    SCENARIO,
    ACTION,
}

/** One pinned shortcut; [id] is a catalog entry id or a native scenario id. */
data class FavoriteRef(val kind: FavoriteKind, val id: String) {
    /** Stable text key used by the UI and by persistence. */
    val key: String get() = Favorites.encode(this)
}

/**
 * Ordered pinned shortcuts for the Home screen. Storage and the ordered list
 * itself are Android-free so the pin/unpin rules can be unit-tested.
 */
object Favorites {
    /** Default native scenario labels pinned on first launch. */
    val DEFAULT_SCENARIO_LABELS = setOf("блэкаут", "blackout", "сел работать", "work mode")

    /** Pins [ref] to the end, or unpins it when it is already pinned. */
    fun toggle(current: List<FavoriteRef>, ref: FavoriteRef): List<FavoriteRef> =
        if (ref in current) current - ref else current + ref

    fun isPinned(current: List<FavoriteRef>, ref: FavoriteRef): Boolean = ref in current

    /**
     * First-launch pins: the "say on the station" action first, then the native
     * scenarios that migrated from the old blackout / work-mode scripts. Only
     * things that actually exist in the catalog are pinned.
     */
    fun defaults(entries: List<ControlEntry>, scenarios: List<Scenario>): List<FavoriteRef> {
        val result = mutableListOf<FavoriteRef>()
        entries.firstOrNull { it.kind == ControlKind.ACTION && it.available }
            ?.let { result += FavoriteRef(FavoriteKind.ACTION, it.id) }
        scenarios
            .filter { it.label.trim().lowercase() in DEFAULT_SCENARIO_LABELS }
            .forEach { result += FavoriteRef(FavoriteKind.SCENARIO, it.id) }
        return result
    }

    fun encode(ref: FavoriteRef): String = "${ref.kind.name.lowercase()}:${ref.id}"

    fun encode(refs: List<FavoriteRef>): List<String> = refs.map(::encode)

    /** Inverse of [encode]; unknown or malformed entries are dropped. */
    fun decode(values: List<String>): List<FavoriteRef> = values.mapNotNull { value ->
        val separator = value.indexOf(':')
        if (separator <= 0 || separator == value.lastIndex) return@mapNotNull null
        val kind = FavoriteKind.entries.firstOrNull {
            it.name.equals(value.substring(0, separator), ignoreCase = true)
        } ?: return@mapNotNull null
        FavoriteRef(kind, value.substring(separator + 1))
    }.distinct()
}

/**
 * Remembers the pinned shortcuts. A null [load] means "never saved yet", so an
 * intentionally emptied list is not replaced by the defaults again.
 */
interface FavoritePreferences {
    suspend fun load(): List<FavoriteRef>?

    suspend fun save(refs: List<FavoriteRef>)
}
