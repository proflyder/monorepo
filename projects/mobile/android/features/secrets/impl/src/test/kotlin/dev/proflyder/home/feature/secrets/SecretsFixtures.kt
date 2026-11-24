package dev.proflyder.home.feature.secrets

import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.SecretEntry
import dev.proflyder.home.core.model.SecretMetadata
import dev.proflyder.home.core.model.SecretRollback
import dev.proflyder.home.core.model.SecretValue
import dev.proflyder.home.core.model.SecretVersionRef
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

internal class FakeSecretsRepository : SecretsRepository {
    val catalogState = MutableStateFlow(SecretsCatalogState(loading = false))
    val stepUpState = MutableStateFlow(false)
    var metadataResult: Outcome<SecretMetadata> = Outcome.Success(metadata())
    var stepUpResult: Outcome<Unit> = Outcome.Success(Unit)
    var revealResult: Outcome<SecretValue> = Outcome.Success(SecretValue(PATH, 3, "token", "s3cret"))
    var updateResult: Outcome<SecretVersionRef> = Outcome.Success(SecretVersionRef(PATH, 4, "token"))
    var rollbackResult: Outcome<SecretRollback> = Outcome.Success(SecretRollback(PATH, 1, 4))
    var deleteResult: Outcome<Unit> = Outcome.Success(Unit)
    var refreshes = 0
    val revealed = mutableListOf<Triple<String, String, Long>>()
    val updates = mutableListOf<Triple<String, String, Long>>()
    val rollbacks = mutableListOf<Pair<Long, Long>>()
    val deletions = mutableListOf<Pair<Long, Long>>()
    var lastTotp: String? = null

    override val catalog: StateFlow<SecretsCatalogState> = catalogState

    override val stepUpActive: StateFlow<Boolean> = stepUpState

    override fun refresh() {
        refreshes++
    }

    override suspend fun metadata(path: String): Outcome<SecretMetadata> = metadataResult

    override suspend fun stepUp(totp: String): Outcome<Unit> {
        lastTotp = totp
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
        totp: String,
    ): Outcome<SecretVersionRef> {
        lastTotp = totp
        updates += Triple(key, value, cas)
        return updateResult
    }

    override suspend fun rollback(
        path: String,
        version: Long,
        cas: Long,
        totp: String,
    ): Outcome<SecretRollback> {
        lastTotp = totp
        rollbacks += version to cas
        return rollbackResult
    }

    override suspend fun delete(path: String, cas: Long, version: Long, totp: String): Outcome<Unit> {
        lastTotp = totp
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

internal class FakeClock(private var nanos: Long = 0) : Clock {
    override fun wallMillis(): Long = nanos / 1_000_000

    override fun elapsedNanos(): Long = nanos

    fun advance(millis: Long) {
        nanos += millis * 1_000_000
    }
}

internal fun forbiddenFailure(): RemoteFailure = RemoteFailure.Server(403, dev.proflyder.home.core.model.ConnectCode.UNKNOWN, "forbidden", "path_forbidden")
