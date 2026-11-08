package dev.proflyder.home.feature.call.telecom

import android.Manifest
import android.annotation.SuppressLint
import android.content.ComponentName
import android.content.Context
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Bundle
import android.telecom.PhoneAccount
import android.telecom.PhoneAccountHandle
import android.telecom.TelecomManager
import androidx.core.content.ContextCompat
import dev.proflyder.home.feature.call.CallController
import dev.proflyder.home.feature.call.CallCoordinator
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesBinding
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

/** Registers the app's self-managed phone account exactly once. */
@SingleIn(AppScope::class)
@Inject
class TelecomAccountRegistry(private val context: Context) {

    fun ensureRegistered(): PhoneAccountHandle? {
        val telecom = context.getSystemService(TelecomManager::class.java) ?: return null
        val handle = PhoneAccountHandle(
            ComponentName(context, HomeConnectionService::class.java),
            ACCOUNT_ID,
        )
        if (telecom.getPhoneAccount(handle) == null) {
            val account = PhoneAccount.builder(handle, ACCOUNT_LABEL)
                .setCapabilities(PhoneAccount.CAPABILITY_SELF_MANAGED)
                .addSupportedUriScheme(PhoneAccount.SCHEME_TEL)
                .build()
            telecom.registerPhoneAccount(account)
        }
        return handle
    }

    private companion object {
        const val ACCOUNT_ID = "home-call"
        const val ACCOUNT_LABEL = "Home"
    }
}

/**
 * Places the outgoing call through [TelecomManager] so the call is a real
 * self-managed call. If Telecom is unavailable (or the account cannot be
 * registered) the call still proceeds directly; only the system call UI and
 * Telecom endpoint routing are lost.
 */
@SingleIn(AppScope::class)
@ContributesBinding(AppScope::class)
@Inject
class AndroidCallCoordinator(
    private val context: Context,
    private val controller: CallController,
    private val registry: TelecomAccountRegistry,
) : CallCoordinator {

    @SuppressLint("MissingPermission")
    override fun startOutgoingCall() {
        // MANAGE_OWN_CALLS is a normal permission (granted at install) and is
        // checked below; lint cannot see through the early return.
        if (!canManageOwnCalls()) {
            controller.placeCall()
            return
        }
        val handle = registry.ensureRegistered()
        val telecom = context.getSystemService(TelecomManager::class.java)
        if (handle == null || telecom == null) {
            controller.placeCall()
            return
        }
        val extras = Bundle().apply {
            putParcelable(TelecomManager.EXTRA_PHONE_ACCOUNT_HANDLE, handle)
        }
        try {
            telecom.placeCall(Uri.fromParts("tel", "home", null), extras)
        } catch (_: Exception) {
            // No dialer permission, no SIM, or Telecom rejected the request:
            // still start the call so the feature keeps working.
            controller.placeCall()
        }
    }

    override fun hangup() = controller.hangup()

    private fun canManageOwnCalls(): Boolean =
        ContextCompat.checkSelfPermission(context, Manifest.permission.MANAGE_OWN_CALLS) ==
            PackageManager.PERMISSION_GRANTED
}
