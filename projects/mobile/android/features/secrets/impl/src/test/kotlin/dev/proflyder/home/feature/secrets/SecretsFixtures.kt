package dev.proflyder.home.feature.secrets

import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.model.DeviceAttestationChallenge
import dev.proflyder.home.core.model.DeviceEnrollment
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SecretEntry
import dev.proflyder.home.core.model.SecretMetadata
import dev.proflyder.home.core.model.SecretOperation
import dev.proflyder.home.core.model.SecretRollback
import dev.proflyder.home.core.model.SecretValue
import dev.proflyder.home.core.model.SecretVersionRef
import dev.proflyder.home.core.model.StepUpChallenge
import dev.proflyder.home.feature.secrets.presentation.DeviceKeyMaterial
import dev.proflyder.home.feature.secrets.presentation.DeviceKeyResult
import dev.proflyder.home.feature.secrets.presentation.DeviceSignatureResult
import dev.proflyder.home.feature.secrets.presentation.SecretsDeviceSigner
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

internal class FakeSecretsRepository : SecretsRepository {
    val catalogState = MutableStateFlow(SecretsCatalogState(loading = false))
    val stepUpState = MutableStateFlow(false)
    var metadataResult: Outcome<SecretMetadata> = Outcome.Success(metadata())
    var deviceResult: Outcome<DeviceEnrollment> = Outcome.Success(DeviceEnrollment(false, null, 0))
    var deviceChallengeResult: Outcome<DeviceAttestationChallenge> =
        Outcome.Success(DeviceAttestationChallenge("attestation-challenge", 900))
    var enrollResult: Outcome<DeviceEnrollment> =
        Outcome.Success(DeviceEnrollment(true, "device-1", 1))
    var stepUpChallengeResult: Outcome<StepUpChallenge> =
        Outcome.Success(StepUpChallenge("challenge-1", "payload-1", 60))
    var stepUpResult: Outcome<Unit> = Outcome.Success(Unit)
    var revealResult: Outcome<SecretValue> = Outcome.Success(SecretValue(PATH, 3, "token", "s3cret"))
    var updateResult: Outcome<SecretVersionRef> = Outcome.Success(SecretVersionRef(PATH, 4, "token"))
    var rollbackResult: Outcome<SecretRollback> = Outcome.Success(SecretRollback(PATH, 1, 4))
    var deleteResult: Outcome<Unit> = Outcome.Success(Unit)
    var refreshes = 0
    var deviceChallenges = 0
    val revealed = mutableListOf<Triple<String, String, Long>>()
    val updates = mutableListOf<Triple<String, String, Long>>()
    val rollbacks = mutableListOf<Pair<Long, Long>>()
    val deletions = mutableListOf<Pair<Long, Long>>()
    val challenges = mutableListOf<SecretOperation>()
    val enrollments = mutableListOf<List<String>>()
    var lastSignature: String? = null
    var lastValueHash: String? = null

    override val catalog: StateFlow<SecretsCatalogState> = catalogState

    override val stepUpActive: StateFlow<Boolean> = stepUpState

    override fun refresh() {
        refreshes++
    }

    override suspend fun metadata(path: String): Outcome<SecretMetadata> = metadataResult

    override suspend fun device(): Outcome<DeviceEnrollment> = deviceResult

    override suspend fun deviceChallenge(): Outcome<DeviceAttestationChallenge> {
        deviceChallenges++
        return deviceChallengeResult
    }

    override suspend fun enroll(
        code: String,
        publicKey: String,
        attestationChallenge: String,
        attestationChain: List<String>,
    ): Outcome<DeviceEnrollment> {
        enrollments += listOf(code, publicKey, attestationChallenge, attestationChain.size.toString())
        return enrollResult
    }

    override suspend fun challenge(
        operation: SecretOperation,
        path: String?,
        key: String?,
        version: Long?,
        cas: Long?,
        valueHash: String?,
    ): Outcome<StepUpChallenge> {
        challenges += operation
        lastValueHash = valueHash
        return stepUpChallengeResult
    }

    override suspend fun stepUp(challenge: String, signature: String): Outcome<Unit> {
        lastSignature = signature
        if (stepUpResult is Outcome.Success) stepUpState.value = true
        return stepUpResult
    }

    override suspend fun reveal(path: String, key: String, version: Long): Outcome<SecretValue> {
        if (!stepUpState.value) return Outcome.Failure(stepUpRequiredFailure())
        revealed += Triple(path, key, version)
        return revealResult
    }

    override suspend fun update(
        path: String,
        key: String,
        value: String,
        cas: Long,
        challenge: String,
        signature: String,
    ): Outcome<SecretVersionRef> {
        lastSignature = signature
        updates += Triple(key, value, cas)
        return updateResult
    }

    override suspend fun rollback(
        path: String,
        version: Long,
        cas: Long,
        challenge: String,
        signature: String,
    ): Outcome<SecretRollback> {
        lastSignature = signature
        rollbacks += version to cas
        return rollbackResult
    }

    override suspend fun delete(
        path: String,
        cas: Long,
        version: Long,
        challenge: String,
        signature: String,
    ): Outcome<Unit> {
        lastSignature = signature
        deletions += cas to version
        return deleteResult
    }

    companion object {
        const val PATH = "k8s/home-assistant/phone-push"

        fun metadata(): SecretMetadata = SecretMetadata(
            path = PATH,
            currentVersion = 3,
            oldestVersion = 1,
            versions = listOf(
                dev.proflyder.home.core.model.SecretVersion(1, "2026-11-01T10:00:00Z", "", false),
                dev.proflyder.home.core.model.SecretVersion(3, "2026-11-02T10:00:00Z", "", false),
            ),
            keys = listOf("token", "endpoint"),
        )

        fun entry(path: String, description: String = "") = SecretEntry(
            path = path,
            description = description,
            consumer = null,
            live = true,
            inCatalog = true,
        )
    }
}

internal class FakeSecretsDeviceSigner(
    private val supported: Boolean = true,
    private val creation: DeviceKeyResult = DeviceKeyResult.Ready(DeviceKeyMaterial("public-key", listOf("certificate"))),
) : SecretsDeviceSigner {
    var cancellationRequested = false
    var requestedPayload: String? = null

    override fun isSupported(): Boolean = supported

    override fun createEnrollmentKey(attestationChallenge: String): DeviceKeyResult = creation

    override fun requestSignature(
        activity: androidx.fragment.app.FragmentActivity,
        payload: String,
        onResult: (DeviceSignatureResult) -> Unit,
    ) {
        requestedPayload = payload
    }

    override fun promoteEnrollmentKey() = Unit

    override fun discardEnrollmentKey() = Unit

    override fun cancel() {
        cancellationRequested = true
    }
}

internal class FakeClock(private var nanos: Long = 0) : Clock {
    override fun wallMillis(): Long = nanos / 1_000_000

    override fun elapsedNanos(): Long = nanos

    fun advance(millis: Long) {
        nanos += millis * 1_000_000
    }
}

internal fun forbiddenFailure(): RemoteFailure = RemoteFailure.Server(403, dev.proflyder.home.core.model.ConnectCode.UNKNOWN, "forbidden", "path_forbidden")
