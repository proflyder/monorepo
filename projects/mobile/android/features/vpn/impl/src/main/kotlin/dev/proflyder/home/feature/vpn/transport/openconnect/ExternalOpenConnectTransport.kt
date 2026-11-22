package dev.proflyder.home.feature.vpn.transport.openconnect

import dev.proflyder.home.core.designsystem.R as DesignR
import android.annotation.SuppressLint
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import androidx.core.net.toUri
import dev.proflyder.home.core.model.VpnProtocolId
import dev.proflyder.home.feature.vpn.VpnTransportInfo
import dev.proflyder.home.feature.vpn.impl.R
import dev.proflyder.home.feature.vpn.transport.TransportStatus
import dev.proflyder.home.feature.vpn.transport.TunnelRequest
import dev.proflyder.home.feature.vpn.transport.VpnTransport
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.SingleIn
import kotlin.time.Duration
import kotlin.time.Duration.Companion.minutes
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.runningFold

@OptIn(ExperimentalCoroutinesApi::class)
@SingleIn(AppScope::class)
@Inject
internal class ExternalOpenConnectTransport(
    private val context: Context,
    private val systemVpn: SystemVpnNetwork,
) : VpnTransport {
    private val launched = MutableStateFlow(false)

    override val info = VpnTransportInfo(ID, R.string.vpn_protocol_openconnect, external = true)

    override val order: Int = 10

    override val needsSystemPermission: Boolean = false

    override val handshakeTimeout: Duration = EXTERNAL_TIMEOUT

    override val status: Flow<TransportStatus> = launched.flatMapLatest { started -> statusWhile(started) }

    override fun installed(): Boolean = resolvable(view("${OpenConnectLink.SCHEME}://")) || openConnectApp() != null

    override suspend fun up(request: TunnelRequest): Boolean {
        val settings = OpenConnectSettings.of(request.settings) ?: return false
        val name = context.getString(DesignR.string.ds_product_name)
        val link = OpenConnectLink.connect(settings, request.route, name) ?: return false
        val intent = view(link).takeIf { resolvable(it) } ?: openConnectApp() ?: return false
        launched.value = launch(intent)
        return launched.value
    }

    override suspend fun down() {
        if (launched.value) launch(view(OpenConnectLink.DISCONNECT))
        launched.value = false
    }

    private fun statusWhile(started: Boolean): Flow<TransportStatus> = when (started) {
        true -> systemVpn.present().runningFold(TransportStatus.Pending, ::next)
        false -> flowOf(TransportStatus.Down)
    }

    private fun next(previous: TransportStatus, present: Boolean): TransportStatus = when {
        present -> TransportStatus.Up(verified = false)
        previous is TransportStatus.Up -> TransportStatus.Down
        else -> previous
    }

    private fun openConnectApp(): Intent? = context.packageManager.getLaunchIntentForPackage(OPENCONNECT_PACKAGE)

    @SuppressLint("QueryPermissionsNeeded")
    private fun resolvable(intent: Intent): Boolean = intent.resolveActivity(context.packageManager) != null

    private fun view(uri: String): Intent = Intent(Intent.ACTION_VIEW, uri.toUri())

    private fun launch(intent: Intent): Boolean =
        runCatching { context.startActivity(intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) }
            .onFailure { failure -> if (failure !is ActivityNotFoundException && failure !is SecurityException) throw failure }
            .isSuccess

    companion object {
        val ID = VpnProtocolId("ocserv")
        private const val OPENCONNECT_PACKAGE = "app.openconnect"
        private val EXTERNAL_TIMEOUT = 3.minutes
    }
}
