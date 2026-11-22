package dev.proflyder.home.core.runtime

import android.content.Context
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class AndroidAppVersion(context: Context) : AppVersion {
    override val name: String by lazy {
        runCatching { context.packageManager.getPackageInfo(context.packageName, 0).versionName }
            .getOrNull()
            .orEmpty()
            .ifBlank { UNKNOWN }
    }

    private companion object {
        const val UNKNOWN = "unknown"
    }
}
