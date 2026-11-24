package dev.proflyder.home.feature.update.data

import com.google.firebase.appdistribution.AppDistributionRelease
import com.google.firebase.appdistribution.FirebaseAppDistribution
import com.google.firebase.appdistribution.FirebaseAppDistributionException
import com.google.firebase.appdistribution.UpdateProgress
import com.google.firebase.appdistribution.UpdateStatus
import dev.proflyder.home.core.operation.suspendRunCatching
import dev.proflyder.home.feature.update.AvailableUpdate
import dev.proflyder.home.feature.update.UpdateFailureKind
import dev.proflyder.home.feature.updatebuild.UpdateBuild
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.channels.ProducerScope
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.tasks.await

@SingleIn(AppScope::class)
@Inject
internal class FirebaseAppUpdater : AppUpdater {
    override val supported: Boolean
        get() = UpdateBuild.APP_DISTRIBUTION

    override suspend fun signIn(): UpdateResult<Unit> =
        sdkCall { FirebaseAppDistribution.getInstance().signInTester().await() }

    override suspend fun check(): UpdateResult<AvailableUpdate> {
        if (!signedIn()) return UpdateResult.Failure(UpdateFailureKind.SIGN_IN_REQUIRED)
        return when (val result = sdkCall { FirebaseAppDistribution.getInstance().checkForNewRelease().await() }) {
            is UpdateResult.Success -> releaseOf(result.value)
            is UpdateResult.Failure -> result
        }
    }

    private fun releaseOf(release: AppDistributionRelease?): UpdateResult<AvailableUpdate> =
        release?.let { UpdateResult.Success(it.toUpdate()) }
            ?: UpdateResult.Failure(UpdateFailureKind.NOT_AVAILABLE)

    override fun update(): Flow<UpdateEvent> = callbackFlow { forwardUpdate() }

    private fun signedIn(): Boolean = runCatching { FirebaseAppDistribution.getInstance().isTesterSignedIn() }
        .getOrDefault(false)

    private suspend fun <T> sdkCall(block: suspend () -> T): UpdateResult<T> =
        suspendRunCatching { block() }.fold(::success, ::failure)

    private fun <T> success(value: T): UpdateResult<T> = UpdateResult.Success(value)

    private fun failure(error: Throwable): UpdateResult<Nothing> = UpdateResult.Failure(kindOf(error))

    private suspend fun ProducerScope<UpdateEvent>.forwardUpdate() {
        val started = runCatching { FirebaseAppDistribution.getInstance().updateApp() }
        val task = started.getOrNull()
        if (task == null) {
            trySend(UpdateEvent.Failed(kindOf(started.exceptionOrNull() ?: IllegalStateException())))
            close()
            return
        }
        task.addOnProgressListener { progress -> trySend(eventOf(progress)) }
        task.addOnSuccessListener { trySend(UpdateEvent.Completed) }
        task.addOnFailureListener { error -> trySend(UpdateEvent.Failed(kindOf(error))) }
        awaitClose()
    }

    private fun eventOf(progress: UpdateProgress): UpdateEvent = when (progress.updateStatus) {
        UpdateStatus.PENDING -> UpdateEvent.Downloading(null)
        UpdateStatus.DOWNLOADING -> UpdateEvent.Downloading(percentOf(progress))
        UpdateStatus.DOWNLOADED -> UpdateEvent.Downloaded
        UpdateStatus.DOWNLOAD_FAILED -> UpdateEvent.Failed(UpdateFailureKind.DOWNLOAD)
        UpdateStatus.INSTALL_CANCELED -> UpdateEvent.Cancelled
        UpdateStatus.INSTALL_FAILED -> UpdateEvent.Failed(UpdateFailureKind.INSTALL)
        UpdateStatus.UPDATE_CANCELED -> UpdateEvent.Cancelled
        UpdateStatus.REDIRECTED_TO_PLAY -> UpdateEvent.Cancelled
        UpdateStatus.NEW_RELEASE_NOT_AVAILABLE -> UpdateEvent.Cancelled
        UpdateStatus.NEW_RELEASE_CHECK_FAILED -> UpdateEvent.Failed(UpdateFailureKind.OFFLINE)
    }

    private fun percentOf(progress: UpdateProgress): Int? {
        val total = progress.apkFileTotalBytes
        if (total <= 0L) return null
        return ((progress.apkBytesDownloaded * PERCENT) / total).toInt().coerceIn(0, PERCENT.toInt())
    }

    private fun kindOf(error: Throwable): UpdateFailureKind =
        when ((error as? FirebaseAppDistributionException)?.errorCode) {
            FirebaseAppDistributionException.Status.AUTHENTICATION_FAILURE -> UpdateFailureKind.SIGN_IN_REQUIRED
            FirebaseAppDistributionException.Status.AUTHENTICATION_CANCELED -> UpdateFailureKind.SIGN_IN_CANCELLED
            FirebaseAppDistributionException.Status.NETWORK_FAILURE -> UpdateFailureKind.OFFLINE
            FirebaseAppDistributionException.Status.UPDATE_NOT_AVAILABLE -> UpdateFailureKind.NOT_AVAILABLE
            FirebaseAppDistributionException.Status.DOWNLOAD_FAILURE -> UpdateFailureKind.DOWNLOAD
            FirebaseAppDistributionException.Status.INSTALLATION_FAILURE -> UpdateFailureKind.INSTALL
            FirebaseAppDistributionException.Status.INSTALLATION_CANCELED -> UpdateFailureKind.INSTALL_CANCELLED
            FirebaseAppDistributionException.Status.NOT_IMPLEMENTED,
            FirebaseAppDistributionException.Status.API_DISABLED,
            -> UpdateFailureKind.UNSUPPORTED
            else -> UpdateFailureKind.UNKNOWN
        }

    private fun AppDistributionRelease.toUpdate(): AvailableUpdate = AvailableUpdate(
        versionName = displayVersion,
        versionCode = versionCode,
        releaseNotes = releaseNotes.orEmpty(),
    )

    private companion object {
        const val PERCENT = 100L
    }
}
