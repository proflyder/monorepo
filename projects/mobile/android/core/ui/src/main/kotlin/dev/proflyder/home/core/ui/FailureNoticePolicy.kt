package dev.proflyder.home.core.ui

import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.isTransientNetwork
import dev.proflyder.home.core.runtime.AppForeground
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject

interface FailureNoticePolicy {
    fun shouldShow(failure: RemoteFailure): Boolean
}

interface FailureNoticePolicyProvider {
    val failureNoticePolicy: FailureNoticePolicy
}

@ContributesBinding(AppScope::class)
@Inject
class ForegroundGraceNoticePolicy(
    private val foreground: AppForeground,
    private val clock: Clock,
) : FailureNoticePolicy {
    override fun shouldShow(failure: RemoteFailure): Boolean =
        !(failure.isTransientNetwork() && inResumeGrace())

    private fun inResumeGrace(): Boolean {
        val foregroundFor = foreground.state.value.foregroundFor(clock.elapsedNanos()) ?: return false
        return foregroundFor in 0..GRACE_NANOS
    }

    companion object {
        const val GRACE_NANOS: Long = 6_000_000_000L
    }
}
