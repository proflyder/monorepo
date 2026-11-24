package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.AccessToken
import dev.proflyder.home.core.model.AppAccess
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.model.CreatedAccessToken
import dev.proflyder.home.core.model.Outcome
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlinx.serialization.Serializable
import retrofit2.http.Body
import retrofit2.http.DELETE
import retrofit2.http.GET
import retrofit2.http.PATCH
import retrofit2.http.POST
import retrofit2.http.Path

@Serializable
internal data class MeDto(
    val name: String = "",
    val modules: List<String> = emptyList(),
    val owner: Boolean = false,
)

@Serializable
internal data class AccessTokenDto(
    val id: String = "",
    val name: String = "",
    val modules: List<String> = emptyList(),
    val owner: Boolean = false,
    val createdUnix: String = "0",
    val updatedUnix: String = "0",
)

@Serializable
internal data class AccessTokensDto(val tokens: List<AccessTokenDto> = emptyList())

@Serializable
internal data class CreateTokenBody(val name: String, val modules: List<String>)

@Serializable
internal data class UpdateTokenBody(val name: String? = null, val modules: List<String>? = null)

@Serializable
internal data class CreatedTokenDto(val token: String = "", val access: AccessTokenDto = AccessTokenDto())

@Serializable
internal data class UpdatedTokenDto(val access: AccessTokenDto = AccessTokenDto())

@Serializable
internal data class RevokedTokenDto(val revoked: Boolean = false)

internal interface AccessHttpApi {
    @GET("v1/me")
    suspend fun me(): Outcome<MeDto>

    @GET("v1/access/tokens")
    suspend fun tokens(): Outcome<AccessTokensDto>

    @POST("v1/access/tokens")
    suspend fun create(@Body body: CreateTokenBody): Outcome<CreatedTokenDto>

    @PATCH("v1/access/tokens/{id}")
    suspend fun update(@Path("id") id: String, @Body body: UpdateTokenBody): Outcome<UpdatedTokenDto>

    @DELETE("v1/access/tokens/{id}")
    suspend fun revoke(@Path("id") id: String): Outcome<RevokedTokenDto>
}

@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class HttpAccessClient(private val http: HomeHttp) : AccessApi {
    private val api: AccessHttpApi by lazy { http.rest(AccessHttpApi::class, Target.Scheduler) }

    override suspend fun me(): Outcome<AppAccess> = api.me().mapResult { it.toModel() }

    override suspend fun tokens(): Outcome<List<AccessToken>> = api.tokens().mapResult { value ->
        value.tokens.map(AccessTokenDto::toModel)
    }

    override suspend fun createToken(
        name: String,
        modules: Set<AppModule>,
    ): Outcome<CreatedAccessToken> = api.create(
        CreateTokenBody(name = name.trim(), modules = modules.wires()),
    ).mapResult { CreatedAccessToken(it.token, it.access.toModel()) }

    override suspend fun updateToken(
        id: String,
        name: String?,
        modules: Set<AppModule>?,
    ): Outcome<AccessToken> = api.update(
        id,
        UpdateTokenBody(name = name?.trim(), modules = modules?.wires()),
    ).mapResult { it.access.toModel() }

    override suspend fun revokeToken(id: String): Outcome<Unit> = api.revoke(id).mapResult { }
}

private fun MeDto.toModel(): AppAccess = AppAccess(
    name = name,
    modules = AppModule.fromWire(modules),
    owner = owner,
)

private fun AccessTokenDto.toModel(): AccessToken = AccessToken(
    id = id,
    name = name,
    modules = AppModule.fromWire(modules),
    owner = owner,
    createdUnix = createdUnix.toLongOrNull() ?: 0L,
    updatedUnix = updatedUnix.toLongOrNull() ?: 0L,
)

private fun Set<AppModule>.wires(): List<String> =
    AppModule.entries.filter { it in this }.map(AppModule::wire)
