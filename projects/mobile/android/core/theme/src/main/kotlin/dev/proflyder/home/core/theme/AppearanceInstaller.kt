package dev.proflyder.home.core.theme

import android.app.Application
import android.content.Context
import dev.proflyder.home.core.runtime.AppStartable
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

@ContributesIntoSet(AppScope::class)
@Inject
class AppearanceInstaller(
    private val context: Context,
    private val appearance: AppearanceController,
) : AppStartable {
    override fun start() {
        appearance.install(context.applicationContext as Application)
    }
}
