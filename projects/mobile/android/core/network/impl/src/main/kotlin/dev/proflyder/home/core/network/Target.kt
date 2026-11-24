package dev.proflyder.home.core.network

import dev.proflyder.home.core.config.ConnectionConfig
import dev.proflyder.home.core.endpoint.ServerEndpoint
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.RemoteFailure

internal sealed class Target(
    val endpointField: Field,
    val connectProtocol: Boolean,
) {
    abstract fun endpointOf(config: ConnectionConfig): ServerEndpoint?

    open val missingEndpoint: RemoteFailure get() = RemoteFailure.Validation(endpointField)

    data object Scheduler : Target(Field.ENDPOINT, connectProtocol = true) {
        override fun endpointOf(config: ConnectionConfig): ServerEndpoint? = config.endpoint

        override val missingEndpoint: RemoteFailure get() = RemoteFailure.NotConfigured
    }

    data object Control : Target(Field.CONTROL_ENDPOINT, connectProtocol = false) {
        override fun endpointOf(config: ConnectionConfig): ServerEndpoint? = config.controlEndpoint
    }

    data object Intercom : Target(Field.INTERCOM_ENDPOINT, connectProtocol = true) {
        override fun endpointOf(config: ConnectionConfig): ServerEndpoint? = config.intercomEndpoint
    }

    data object Secrets : Target(Field.SECRETS_ENDPOINT, connectProtocol = false) {
        override fun endpointOf(config: ConnectionConfig): ServerEndpoint? = config.secretsEndpoint
    }
}
