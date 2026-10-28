package dev.proflyder.home.core.ui

import android.content.Context
import dev.proflyder.home.core.model.Field
import dev.proflyder.home.core.model.RemoteFailure

/** Turns domain failures into honest, non-secret user-facing text. */
fun RemoteFailure.userMessage(context: Context): String = when (this) {
    RemoteFailure.NotConfigured -> context.getString(R.string.ui_error_not_configured)
    RemoteFailure.Unauthorized -> context.getString(R.string.ui_error_unauthorized)
    RemoteFailure.Timeout -> context.getString(R.string.ui_error_timeout)
    is RemoteFailure.Offline -> context.getString(R.string.ui_error_offline)
    is RemoteFailure.Tls -> context.getString(R.string.ui_error_tls)
    is RemoteFailure.Server ->
        context.getString(R.string.ui_error_server, code, message)
    is RemoteFailure.ProtocolFailure -> context.getString(R.string.ui_error_protocol)
    is RemoteFailure.Storage -> context.getString(R.string.ui_error_storage, detail)
    is RemoteFailure.Validation -> when (field) {
        Field.ENDPOINT -> context.getString(R.string.ui_validation_endpoint)
        Field.TOKEN -> context.getString(R.string.ui_validation_token)
        Field.TIMEOUT -> context.getString(R.string.ui_validation_timeout)
        Field.LABEL -> context.getString(R.string.ui_validation_label)
        Field.TIME -> context.getString(R.string.ui_validation_time)
        Field.DATE -> context.getString(R.string.ui_validation_date)
        Field.TIMEZONE -> context.getString(R.string.ui_validation_timezone)
        Field.WEEKDAYS -> context.getString(R.string.ui_validation_weekdays)
        Field.REPEAT -> context.getString(R.string.ui_validation_repeat)
        Field.DURATION -> context.getString(R.string.ui_validation_duration)
    }
}
