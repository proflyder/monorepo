package dev.proflyder.home.feature.push

import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesTo

@ContributesTo(AppScope::class)
interface PushInjector {
    fun inject(service: HomeMessagingService)

    fun inject(service: FindPhoneService)

    fun inject(activity: FindPhoneActivity)
}
