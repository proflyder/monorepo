package dev.proflyder.home.feature.call

import android.content.Context
import android.content.Intent
import android.widget.Toast
import dev.proflyder.home.core.config.ConnectionSettingsStore
import dev.proflyder.home.feature.call.impl.R
import dev.proflyder.home.feature.call.presentation.CallActivity
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

/** Opens the lock-screen call screen from the Home quick action. */
@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class AndroidCallLauncher(
    private val context: Context,
    private val settings: ConnectionSettingsStore,
) : CallLauncher {
    override fun openCallScreen() {
        if (!settings.config.value.isIntercomConfigured) {
            Toast.makeText(context, R.string.call_error_not_configured, Toast.LENGTH_LONG).show()
            return
        }
        val intent = Intent(context, CallActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            .putExtra(CallActivity.EXTRA_START_CALL, true)
        context.startActivity(intent)
    }
}
