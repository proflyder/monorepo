package dev.proflyder.home.feature.call

import dev.proflyder.home.feature.call.telecom.HomeConnectionService
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesTo

@ContributesTo(AppScope::class)
interface CallInjector {
    fun inject(service: CallForegroundService)

    fun inject(service: HomeConnectionService)
}
