package dev.proflyder.home.feature.call

import android.content.Context
import android.content.Intent
import dev.proflyder.home.feature.call.presentation.CallActivity
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

/** Opens the lock-screen call screen from the Home quick action. */
@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class AndroidCallLauncher(private val context: Context) : CallLauncher {
    override fun openCallScreen() {
        val intent = Intent(context, CallActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            .putExtra(CallActivity.EXTRA_START_CALL, true)
        context.startActivity(intent)
    }
}
