package dev.proflyder.home.core.ui

import android.annotation.SuppressLint
import android.content.Context

/**
 * Resolves a server `LocalizedText.key` to its local resource.
 *
 * Resources are named mechanically after the key: `scheduler.error.not_found`
 * is `@string/server_scheduler_error_not_found` (prefix `server_`, dots become
 * underscores), so adding a server key never needs a Kotlin change. Unknown keys
 * return null so the caller can fall back to the server's legacy text and then
 * to a generic message.
 *
 * Android substitutes arguments positionally, so [args] are applied ordered by
 * name: a server template must name its arguments so that sorted names match the
 * `%1$s`, `%2$s`, ... order in `strings.xml`.
 */
@SuppressLint("DiscouragedApi") // Dynamic server keys, not static resource ids.
fun serverText(context: Context, key: String, args: Map<String, String>): String? {
    val name = "server_" + key.replace('.', '_')
    val id = context.resources.getIdentifier(name, "string", context.packageName)
    return when (id) {
        0 -> null
        else -> context.getString(id, *args.toSortedMap().values.toTypedArray())
    }
}
