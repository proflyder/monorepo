package dev.proflyder.home.feature.vpn.tunnel

import dev.proflyder.home.core.runtime.IntentToken
import dev.proflyder.home.feature.push.PushIntents
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class VpnNotificationIntentTest {
    @Test
    fun theVpnNotificationCarriesTheInternalDeepLinkAndToken() {
        val context = RuntimeEnvironment.getApplication()

        val intent = VpnNotifications.decorate(context, android.content.Intent())

        assertEquals(VpnDestinationRoute.ROUTE, intent?.getStringExtra(PushIntents.EXTRA_DEEP_LINK))
        assertEquals(
            IntentToken(context).current(),
            intent?.getStringExtra(PushIntents.EXTRA_DEEP_LINK_TOKEN),
        )
    }
}
