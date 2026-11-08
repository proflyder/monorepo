package dev.proflyder.home.feature.call

import dev.proflyder.home.core.network.IntercomClient
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineScope

/**
 * Binds the JVM-testable [CallManager] into the Metro graph. [CallManager] lives
 * in the API module and deliberately has no DI annotations, so its constructor
 * stays a plain test seam.
 */
@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class AppCallController(
    client: IntercomClient,
    mediaFactory: CallMediaFactory,
    scope: CoroutineScope,
) : CallController by CallManager(
    client = client,
    mediaFactory = mediaFactory,
    scope = scope,
)
