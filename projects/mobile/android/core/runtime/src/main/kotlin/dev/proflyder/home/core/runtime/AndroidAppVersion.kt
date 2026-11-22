package dev.proflyder.home.core.runtime

import android.content.Context
import android.content.pm.ApplicationInfo
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class AndroidAppVersion(private val context: Context) : AppVersion {
    override val name: String by lazy {
        runCatching { context.packageManager.getPackageInfo(context.packageName, 0).versionName }
            .getOrNull()
            .orEmpty()
            .ifBlank { UNKNOWN }
    }

    override val debuggable: Boolean
        get() = context.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE != 0

    private companion object {
        const val UNKNOWN = "unknown"
    }
}
