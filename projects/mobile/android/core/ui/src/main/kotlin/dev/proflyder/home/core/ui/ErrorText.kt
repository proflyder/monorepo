package dev.proflyder.home.core.ui

import android.content.Context
import dev.proflyder.home.core.model.ConnectCode
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.RemoteFailure
import dev.proflyder.home.core.model.StorageReason

fun RemoteFailure.userMessage(context: Context): String = when (this) {
    RemoteFailure.NotConfigured -> context.getString(R.string.ui_error_not_configured)
    RemoteFailure.Unauthorized -> context.getString(R.string.ui_error_unauthorized)
    is RemoteFailure.ModuleForbidden -> context.getString(R.string.ui_error_no_access)
    RemoteFailure.Timeout -> context.getString(R.string.ui_error_timeout)
    is RemoteFailure.Offline -> context.getString(R.string.ui_error_offline)
    is RemoteFailure.Tls -> context.getString(R.string.ui_error_tls)
    is RemoteFailure.Server -> serverText(context, textKey, textArgs) ?: statusMessage(context)
    is RemoteFailure.ProtocolFailure -> context.getString(R.string.ui_error_protocol)
    is RemoteFailure.Storage -> context.getString(reason.textRes())
    RemoteFailure.RemoteConfig -> context.getString(R.string.ui_error_remote_config)
    is RemoteFailure.Validation -> when (field) {
        Field.ENDPOINT -> context.getString(R.string.ui_validation_endpoint)
        Field.CONTROL_ENDPOINT -> context.getString(R.string.ui_validation_control_endpoint)
        Field.INTERCOM_ENDPOINT -> context.getString(R.string.ui_validation_intercom_endpoint)
        Field.SECRETS_ENDPOINT -> context.getString(R.string.ui_validation_secrets_endpoint)
        Field.TOKEN -> context.getString(R.string.ui_validation_token)
        Field.TIMEOUT -> context.getString(R.string.ui_validation_timeout)
        Field.LABEL -> context.getString(R.string.ui_validation_label)
        Field.TIME -> context.getString(R.string.ui_validation_time)
        Field.DATE -> context.getString(R.string.ui_validation_date)
        Field.TIMEZONE -> context.getString(R.string.ui_validation_timezone)
        Field.WEEKDAYS -> context.getString(R.string.ui_validation_weekdays)
        Field.REPEAT -> context.getString(R.string.ui_validation_repeat)
        Field.DURATION -> context.getString(R.string.ui_validation_duration)
        Field.DESCRIPTION -> context.getString(R.string.ui_validation_description)
        Field.TRIGGERS -> context.getString(R.string.ui_validation_triggers)
        Field.CONDITIONS -> context.getString(R.string.ui_validation_conditions)
        Field.STEPS -> context.getString(R.string.ui_validation_steps)
        Field.SPEAK -> context.getString(R.string.ui_validation_speak)
        Field.ENTITY -> context.getString(R.string.ui_validation_entity)
        Field.UNSUPPORTED -> context.getString(R.string.ui_validation_unsupported)
    }
}

private fun RemoteFailure.Server.statusMessage(context: Context): String {
    val known = statusText()
    return when {
        known != null -> context.getString(known, serverCode(context))
        message.isNotBlank() -> context.getString(R.string.ui_error_server, serverCode(context), message)
        else -> context.getString(R.string.ui_error_server_generic, serverCode(context))
    }
}

private fun RemoteFailure.Server.statusText(): Int? = when {
    code == ConnectCode.NOT_FOUND || httpStatus == HTTP_NOT_FOUND -> R.string.ui_error_not_found
    code == ConnectCode.UNIMPLEMENTED || httpStatus == HTTP_NOT_IMPLEMENTED -> R.string.ui_error_unsupported
    code == ConnectCode.RESOURCE_EXHAUSTED || httpStatus == HTTP_TOO_MANY_REQUESTS -> R.string.ui_error_too_many
    httpStatus >= HTTP_SERVER_ERROR -> R.string.ui_error_server_failed
    else -> null
}

private const val HTTP_NOT_FOUND = 404
private const val HTTP_TOO_MANY_REQUESTS = 429
private const val HTTP_SERVER_ERROR = 500
private const val HTTP_NOT_IMPLEMENTED = 501

private fun RemoteFailure.Server.serverCode(context: Context): String =
    reason.ifBlank { context.getString(R.string.ui_error_http_status, httpStatus) }

private fun StorageReason.textRes(): Int = when (this) {
    StorageReason.TOKEN_ENCRYPTION -> R.string.ui_error_storage_token
    StorageReason.SETTINGS_WRITE -> R.string.ui_error_storage_settings
    StorageReason.ATTACHMENT_MISSING -> R.string.ui_error_storage_attachment
}
