package dev.proflyder.home.feature.call.telecom

import android.telecom.Connection
import android.telecom.ConnectionRequest
import android.telecom.ConnectionService
import android.telecom.PhoneAccountHandle
import dev.proflyder.home.feature.call.CallEntryPoint
import dev.proflyder.home.feature.call.impl.R
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel

class HomeConnectionService : ConnectionService() {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    override fun onCreateOutgoingConnection(
        account: PhoneAccountHandle?,
        request: ConnectionRequest?,
    ): Connection {
        val dependencies = CallEntryPoint.require()
        val connection = HomeConnection(
            controller = dependencies.controller,
            bridge = dependencies.telecomBridge,
            scope = scope,
            callerName = getString(R.string.call_account_label),
        )
        connection.startOutgoing()
        return connection
    }

    override fun onCreateOutgoingConnectionFailed(
        account: PhoneAccountHandle?,
        request: ConnectionRequest?,
    ) = Unit

    override fun onCreateIncomingConnection(
        account: PhoneAccountHandle?,
        request: ConnectionRequest?,
    ): Connection {
        return Connection.createCanceledConnection()
    }

    override fun onCreateIncomingConnectionFailed(
        account: PhoneAccountHandle?,
        request: ConnectionRequest?,
    ) = Unit

    override fun onDestroy() {
        scope.cancel()
        super.onDestroy()
    }
}
