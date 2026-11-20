package dev.proflyder.home.core.ui

import android.annotation.SuppressLint
import android.content.Context

@SuppressLint("DiscouragedApi")
fun serverText(context: Context, key: String, args: Map<String, String>): String? {
    val name = "server_" + key.replace('.', '_')
    val id = context.resources.getIdentifier(name, "string", context.packageName)
    return when (id) {
        0 -> null
        else -> context.getString(id, *args.toSortedMap().values.toTypedArray())
    }
}
