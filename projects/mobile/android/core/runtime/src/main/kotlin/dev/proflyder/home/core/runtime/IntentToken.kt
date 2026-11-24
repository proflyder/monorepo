package dev.proflyder.home.core.runtime

import android.content.Context
import androidx.core.content.edit
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.security.MessageDigest
import java.util.UUID

@SingleIn(AppScope::class)
@Inject
class IntentToken(private val context: Context) {
    fun current(): String {
        val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
        preferences.getString(KEY, null)?.let { return it }
        val created = UUID.randomUUID().toString()
        preferences.edit(commit = true) { putString(KEY, created) }
        return created
    }

    fun matches(candidate: String?): Boolean {
        if (candidate.isNullOrEmpty()) return false
        return MessageDigest.isEqual(candidate.toByteArray(), current().toByteArray())
    }

    private companion object {
        const val PREFERENCES = "home_internal_intent"
        const val KEY = "token"
    }
}
