package dev.proflyder.home.core.network

import dev.proflyder.home.core.model.Outcome
import dev.proflyder.home.core.model.RemoteFailure
import java.lang.reflect.GenericArrayType
import java.lang.reflect.ParameterizedType
import java.lang.reflect.Type
import java.lang.reflect.TypeVariable
import java.lang.reflect.WildcardType
import kotlinx.serialization.json.Json
import retrofit2.Call
import retrofit2.CallAdapter
import retrofit2.Response
import retrofit2.Retrofit

/**
 * Lets an API method declare `Outcome<T>` instead of `Response<T>`.
 *
 * Success bodies are already decoded by the kotlinx converter. Anything else
 * becomes one of the existing [RemoteFailure] values: Connect error envelopes
 * (including their `LocalizedText` detail) through [parseConnectFailure],
 * transport errors through [toRemoteFailure], malformed bodies through
 * [toCallFailure]. Clients therefore never branch on HTTP status themselves.
 */
class OutcomeCallAdapterFactory(private val json: Json) : CallAdapter.Factory() {
    override fun get(
        returnType: Type,
        annotations: Array<out Annotation>,
        retrofit: Retrofit,
    ): CallAdapter<*, *>? {
        if (rawType(returnType) != Outcome::class.java) return null
        val parameterized = returnType as? ParameterizedType ?: return null
        val bodyType = bodyType(parameterized.actualTypeArguments[0])
        val callType = SimpleParameterizedType(Call::class.java, arrayOf(bodyType))
        @Suppress("UNCHECKED_CAST")
        val delegate = retrofit.nextCallAdapter(this, callType, annotations) as CallAdapter<Any, Call<Any>>
        return OutcomeCallAdapter(delegate, json)
    }

    /** `T` from `Outcome<T>`, unwrapping wildcards from Kotlin covariance. */
    private fun bodyType(type: Type): Type = when (type) {
        is WildcardType -> bodyType(type.upperBounds.first())
        is TypeVariable<*> -> bodyType(type.bounds.first())
        else -> type
    }

    private fun rawType(type: Type): Class<*>? = when (type) {
        is Class<*> -> type
        is ParameterizedType -> type.rawType as? Class<*>
        is GenericArrayType -> rawType(type.genericComponentType)
        is WildcardType -> rawType(type.upperBounds.first())
        is TypeVariable<*> -> rawType(type.bounds.first())
        else -> null
    }
}

private class SimpleParameterizedType(
    private val raw: Type,
    private val arguments: Array<Type>,
) : ParameterizedType {
    override fun getActualTypeArguments(): Array<Type> = arguments

    override fun getRawType(): Type = raw

    override fun getOwnerType(): Type? = null
}

private class OutcomeCallAdapter<T>(
    private val delegate: CallAdapter<T, Call<T>>,
    private val json: Json,
) : CallAdapter<T, Outcome<T>> {
    override fun responseType(): Type = delegate.responseType()

    override fun adapt(call: Call<T>): Outcome<T> {
        val response = runCatching { delegate.adapt(call).execute() }.getOrElse { failure ->
            return Outcome.Failure(failure.toCallFailure())
        }
        return when {
            response.isSuccessful -> success(response)
            response.code() in 300..399 ->
                Outcome.Failure(RemoteFailure.ProtocolFailure("HTTP ${response.code()} redirect refused"))
            response.code() == 401 || response.code() == 403 -> Outcome.Failure(RemoteFailure.Unauthorized)
            else -> Outcome.Failure(parseConnectFailure(response.code(), errorText(response), json))
        }
    }

    private fun success(response: Response<T>): Outcome<T> {
        val body = response.body()
        return if (body == null) {
            Outcome.Failure(RemoteFailure.ProtocolFailure("empty response body"))
        } else {
            Outcome.Success(body)
        }
    }

    private fun errorText(response: Response<T>): String =
        runCatching { response.errorBody()?.use { it.readBounded() }.orEmpty() }
            .getOrDefault("")
}
