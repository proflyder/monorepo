package dev.proflyder.home.feature.push

import android.content.Intent
import dev.proflyder.home.core.runtime.IntentToken
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class PushDeepLinkSourceTest {
    private val token = IntentToken(RuntimeEnvironment.getApplication())
    private val source = PushDeepLinkSource(token)

    @Test
    fun anExtraWithoutTheInternalTokenIsIgnored() {
        val intent = Intent().putExtra(PushIntents.EXTRA_DEEP_LINK, "secrets/device")

        assertNull(source.take(intent))
        assertNull(intent.getStringExtra(PushIntents.EXTRA_DEEP_LINK))
    }

    @Test
    fun anExtraWithAForeignTokenIsIgnored() {
        val intent = Intent()
            .putExtra(PushIntents.EXTRA_DEEP_LINK, "secrets/device")
            .putExtra(PushIntents.EXTRA_DEEP_LINK_TOKEN, "foreign")

        assertNull(source.take(intent))
    }

    @Test
    fun anInternalExtraIsAcceptedAndConsumed() {
        val value = token.current()
        val intent = Intent()
            .putExtra(PushIntents.EXTRA_DEEP_LINK, "secrets/device")
            .putExtra(PushIntents.EXTRA_DEEP_LINK_TOKEN, value)

        assertEquals(value, intent.getStringExtra(PushIntents.EXTRA_DEEP_LINK_TOKEN))
        org.junit.Assert.assertTrue(token.matches(value))
        val raw = intent.getStringExtra(PushIntents.EXTRA_DEEP_LINK)
        assertEquals("secrets/device", raw)
        assertEquals("secrets/device", PushDeepLink.parse("home://$raw")?.route)
        assertEquals("secrets/device", source.take(intent))
        assertNull(intent.getStringExtra(PushIntents.EXTRA_DEEP_LINK_TOKEN))
    }

    @Test
    fun anInvalidRouteIsRejectedEvenWithTheToken() {
        val intent = Intent()
            .putExtra(PushIntents.EXTRA_DEEP_LINK, "https://example.test/secrets")
            .putExtra(PushIntents.EXTRA_DEEP_LINK_TOKEN, token.current())

        assertNull(source.take(intent))
    }
}
