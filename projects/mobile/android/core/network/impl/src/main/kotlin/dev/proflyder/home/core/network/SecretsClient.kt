package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.DeviceAttestationChallenge
import dev.proflyder.home.core.model.DeviceEnrollment
import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.SecretCatalog
import dev.proflyder.home.core.model.SecretEntry
import dev.proflyder.home.core.model.SecretMetadata
import dev.proflyder.home.core.model.SecretOperation
import dev.proflyder.home.core.model.SecretRollback
import dev.proflyder.home.core.model.SecretValue
import dev.proflyder.home.core.model.SecretVersion
import dev.proflyder.home.core.model.SecretVersionRef
import dev.proflyder.home.core.model.StepUpChallenge
import dev.proflyder.home.core.model.StepUpSession
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import java.net.URLEncoder
import kotlinx.serialization.Serializable
import retrofit2.http.Body
import retrofit2.http.DELETE
import retrofit2.http.GET
import retrofit2.http.Header
import retrofit2.http.POST
import retrofit2.http.Path
import retrofit2.http.Query

@Serializable
internal data class SecretEntryDto(
    val path: String = "",
    val description: String = "",
    val consumer: String? = null,
    val live: Boolean = false,
    val inCatalog: Boolean = false,
)

@Serializable
internal data class SecretCatalogDto(
    val entries: List<SecretEntryDto> = emptyList(),
    val truncated: Boolean = false,
)

@Serializable
internal data class SecretVersionDto(
    val version: Long = 0,
    val createdTime: String = "",
    val deletionTime: String = "",
    val destroyed: Boolean = false,
)

@Serializable
internal data class SecretMetadataDto(
    val path: String = "",
    val currentVersion: Long = 0,
    val oldestVersion: Long = 0,
    val versions: List<SecretVersionDto> = emptyList(),
    val keys: List<String> = emptyList(),
)

@Serializable
internal data class SecretRevealDto(
    val path: String = "",
    val version: Long = 0,
    val key: String = "",
    val value: String = "",
)

@Serializable
internal data class SecretStepUpDto(
    val session: String = "",
    val expiresIn: Long = 0,
)

@Serializable
internal data class SecretUpdateDto(
    val path: String = "",
    val version: Long = 0,
    val key: String = "",
)

@Serializable
internal data class SecretRollbackDto(
    val path: String = "",
    val fromVersion: Long = 0,
    val version: Long = 0,
)

@Serializable
internal data class SecretDeleteDto(
    val path: String = "",
    val deleted: Boolean = false,
    val version: Long = 0,
)

@Serializable
internal data class SecretDeviceDto(
    val enrolled: Boolean = false,
    val deviceId: String? = null,
    val enrolledAt: Long = 0,
)

@Serializable
internal data class SecretDeviceChallengeDto(
    val challenge: String = "",
    val expiresIn: Long = 0,
)

@Serializable
internal data class SecretDeviceEnrollBody(
    val code: String,
    val publicKey: String,
    val attestationChallenge: String,
    val attestationChain: List<String>,
)

@Serializable
internal data class SecretChallengeBody(
    val action: String,
    val path: String? = null,
    val key: String? = null,
    val version: Long? = null,
    val cas: Long? = null,
)

@Serializable
internal data class SecretChallengeDto(
    val challenge: String = "",
    val payload: String = "",
    val expiresIn: Long = 0,
)

@Serializable
internal data class SecretStepUpBody(val challenge: String, val signature: String)

@Serializable
internal data class SecretRevealBody(val key: String, val version: Long)

@Serializable
internal data class SecretUpdateBody(val key: String, val value: String, val cas: Long)

@Serializable
internal data class SecretRollbackBody(val version: Long, val cas: Long)

internal interface SecretsHttpApi {
    @DetailedServerErrors
    @GET("v1/secrets")
    suspend fun catalog(): Outcome<SecretCatalogDto>

    @DetailedServerErrors
    @GET("v1/secrets/meta/{path}")
    suspend fun metadata(@Path(value = "path", encoded = true) path: String): Outcome<SecretMetadataDto>

    @DetailedServerErrors
    @GET("v1/device")
    suspend fun device(): Outcome<SecretDeviceDto>

    @DetailedServerErrors
    @POST("v1/device/challenge")
    suspend fun deviceChallenge(): Outcome<SecretDeviceChallengeDto>

    @DetailedServerErrors
    @POST("v1/device/enroll")
    suspend fun enroll(@Body body: SecretDeviceEnrollBody): Outcome<SecretDeviceDto>

    @DetailedServerErrors
    @POST("v1/step-up/challenge")
    suspend fun challenge(@Body body: SecretChallengeBody): Outcome<SecretChallengeDto>

    @DetailedServerErrors
    @POST("v1/step-up")
    suspend fun stepUp(@Body body: SecretStepUpBody): Outcome<SecretStepUpDto>

    @DetailedServerErrors
    @POST("v1/secrets/reveal/{path}")
    suspend fun reveal(
        @Path(value = "path", encoded = true) path: String,
        @Header("X-Step-Up-Session") session: String,
        @Body body: SecretRevealBody,
    ): Outcome<SecretRevealDto>

    @DetailedServerErrors
    @POST("v1/secrets/update/{path}")
    suspend fun update(
        @Path(value = "path", encoded = true) path: String,
        @Header("X-Step-Up-Challenge") challenge: String,
        @Header("X-Step-Up-Signature") signature: String,
        @Body body: SecretUpdateBody,
    ): Outcome<SecretUpdateDto>

    @DetailedServerErrors
    @POST("v1/secrets/rollback/{path}")
    suspend fun rollback(
        @Path(value = "path", encoded = true) path: String,
        @Header("X-Step-Up-Challenge") challenge: String,
        @Header("X-Step-Up-Signature") signature: String,
        @Body body: SecretRollbackBody,
    ): Outcome<SecretRollbackDto>

    @DetailedServerErrors
    @DELETE("v1/secrets/{path}")
    suspend fun delete(
        @Path(value = "path", encoded = true) path: String,
        @Query("cas") cas: Long,
        @Query("version") version: Long,
        @Header("X-Step-Up-Challenge") challenge: String,
        @Header("X-Step-Up-Signature") signature: String,
    ): Outcome<SecretDeleteDto>
}

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class HttpSecretsClient(private val http: HomeHttp) : SecretsApi {
    private val api: SecretsHttpApi by lazy {
        http.rest(SecretsHttpApi::class, Target.Secrets, inspected = false)
    }

    override suspend fun catalog(): Outcome<SecretCatalog> = api.catalog().mapResult { value ->
        SecretCatalog(value.entries.map(SecretEntryDto::toModel), value.truncated)
    }

    override suspend fun metadata(path: String): Outcome<SecretMetadata> =
        api.metadata(encodedPath(path)).mapResult(SecretMetadataDto::toModel)

    override suspend fun device(): Outcome<DeviceEnrollment> = api.device().mapResult { value ->
        DeviceEnrollment(value.enrolled, value.deviceId, value.enrolledAt)
    }

    override suspend fun deviceChallenge(): Outcome<DeviceAttestationChallenge> =
        api.deviceChallenge().mapResult { value ->
            DeviceAttestationChallenge(value.challenge, value.expiresIn)
        }

    override suspend fun enroll(
        code: String,
        publicKey: String,
        attestationChallenge: String,
        attestationChain: List<String>,
    ): Outcome<DeviceEnrollment> =
        api.enroll(SecretDeviceEnrollBody(code.trim(), publicKey, attestationChallenge, attestationChain))
            .mapResult { value -> DeviceEnrollment(value.enrolled, value.deviceId, value.enrolledAt) }

    override suspend fun challenge(
        operation: SecretOperation,
        path: String?,
        key: String?,
        version: Long?,
        cas: Long?,
    ): Outcome<StepUpChallenge> =
        api.challenge(SecretChallengeBody(operation.wire, path, key, version, cas))
            .mapResult { value -> StepUpChallenge(value.challenge, value.payload, value.expiresIn) }

    override suspend fun stepUp(challenge: String, signature: String): Outcome<StepUpSession> =
        api.stepUp(SecretStepUpBody(challenge, signature))
            .mapResult { StepUpSession(it.session, it.expiresIn) }

    override suspend fun reveal(
        path: String,
        key: String,
        version: Long,
        session: String,
    ): Outcome<SecretValue> = api.reveal(encodedPath(path), session, SecretRevealBody(key, version))
        .mapResult { SecretValue(it.path, it.version, it.key, it.value) }

    override suspend fun update(
        path: String,
        key: String,
        value: String,
        cas: Long,
        challenge: String,
        signature: String,
    ): Outcome<SecretVersionRef> =
        api.update(encodedPath(path), challenge, signature, SecretUpdateBody(key, value, cas))
            .mapResult { SecretVersionRef(it.path, it.version, it.key) }

    override suspend fun rollback(
        path: String,
        version: Long,
        cas: Long,
        challenge: String,
        signature: String,
    ): Outcome<SecretRollback> =
        api.rollback(encodedPath(path), challenge, signature, SecretRollbackBody(version, cas))
            .mapResult { SecretRollback(it.path, it.fromVersion, it.version) }

    override suspend fun delete(
        path: String,
        cas: Long,
        version: Long,
        challenge: String,
        signature: String,
    ): Outcome<Unit> =
        api.delete(encodedPath(path), cas, version, challenge, signature).mapResult { }
}

private fun SecretEntryDto.toModel(): SecretEntry = SecretEntry(
    path = path,
    description = description,
    consumer = consumer,
    live = live,
    inCatalog = inCatalog,
)

private fun SecretMetadataDto.toModel(): SecretMetadata = SecretMetadata(
    path = path,
    currentVersion = currentVersion,
    oldestVersion = oldestVersion,
    versions = versions.map { SecretVersion(it.version, it.createdTime, it.deletionTime, it.destroyed) },
    keys = keys,
)

internal fun encodedPath(path: String): String =
    path.trim('/').split('/').joinToString("/") { URLEncoder.encode(it, Charsets.UTF_8.name()).replace("+", "%20") }
