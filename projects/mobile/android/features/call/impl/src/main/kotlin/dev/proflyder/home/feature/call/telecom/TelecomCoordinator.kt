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
import dev.proflyder.home.feature.call.impl.R
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn

internal interface CallCoordinator {
    fun startOutgoingCall()
}

@SingleIn(AppScope::class)
@Inject
internal class TelecomAccountRegistry(private val context: Context) {

    fun ensureRegistered(): PhoneAccountHandle? {
        val telecom = context.getSystemService(TelecomManager::class.java) ?: return null
        val handle = PhoneAccountHandle(
            ComponentName(context, HomeConnectionService::class.java),
            ACCOUNT_ID,
        )
        return runCatching { telecom.registerPhoneAccount(account(handle)) }
            .map { handle }
            .getOrNull()
    }

    private fun account(handle: PhoneAccountHandle): PhoneAccount =
        PhoneAccount.builder(handle, context.getString(R.string.call_account_label))
            .setCapabilities(PhoneAccount.CAPABILITY_SELF_MANAGED)
            .addSupportedUriScheme(PhoneAccount.SCHEME_TEL)
            .build()

    private companion object {
        const val ACCOUNT_ID = "home-call"
    }
}

@SingleIn(AppScope::class)
@Inject
internal class AndroidCallCoordinator(
    private val context: Context,
    private val controller: CallController,
    private val registry: TelecomAccountRegistry,
) : CallCoordinator {

    @SuppressLint("MissingPermission")
    override fun startOutgoingCall() {
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
        runCatching {
            telecom.placeCall(Uri.fromParts("tel", "home", null), extras)
        }.onFailure { failure ->
            if (failure !is Exception) throw failure
            controller.placeCall()
        }
    }

    private fun canManageOwnCalls(): Boolean =
        ContextCompat.checkSelfPermission(context, Manifest.permission.MANAGE_OWN_CALLS) ==
            PackageManager.PERMISSION_GRANTED
}
