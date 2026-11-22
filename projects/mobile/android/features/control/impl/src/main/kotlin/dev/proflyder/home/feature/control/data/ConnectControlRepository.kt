package dev.proflyder.home.feature.control.data

import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.network.ControlCatalogClient
import dev.proflyder.home.core.network.SchedulerClient
import dev.proflyder.home.core.runtime.AppForeground
import dev.proflyder.home.feature.control.ControlRepository
import dev.proflyder.home.feature.control.VoiceProfileSwitch
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import dev.zacsweers.metro.binding
import kotlinx.coroutines.CoroutineScope

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class, binding = binding<ControlRepository>())
@ContributesBinding(AppScope::class, binding = binding<VoiceProfileSwitch>())
class ConnectControlRepository private constructor(
    catalog: CatalogRepository,
) : ControlRepository by catalog, VoiceProfileSwitch by CatalogVoiceProfileSwitch(catalog) {
    @Inject
    constructor(
        settings: ConnectionSettingsStore,
        foreground: AppForeground,
        catalogClient: ControlCatalogClient,
        scheduler: SchedulerClient,
        clock: Clock,
        scope: CoroutineScope,
    ) : this(CatalogRepository.create(settings, foreground, catalogClient, scheduler, clock, scope))
}
