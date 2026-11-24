package dev.proflyder.home.core.navigation

import androidx.fragment.app.Fragment
import dev.proflyder.home.core.access.AppAccessState
import dev.proflyder.home.core.model.AppAccess
import dev.proflyder.home.core.model.AppModule
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class NotificationTargetsTest {
    private val targets = NotificationTargets(
        FeatureRegistry(
            listOf(
                timeFeature(),
                chatFeature(),
                TestEntry(id = "alarms", module = AppModule.ALARMS_TIMERS, rootDestination = TestDestination("alarms", "alarms/list")),
                TestEntry(id = "timers", module = AppModule.ALARMS_TIMERS, rootDestination = TestDestination("timers", "timers/list")),
                TestEntry(id = "reminders", module = AppModule.ALARMS_TIMERS, rootDestination = TestDestination("reminders", "reminders/list")),
                secretsFeature(),
                TestEntry(id = "access", module = AppModule.ADMIN, notificationLink = false, rootDestination = TestDestination("access", "access/tokens")),
            ),
        ),
    )

    @Test
    fun aChatMessageOpensChatAndKeepsTheMessageFocus() {
        assertEquals(
            NotificationTarget(Route.Link("chat/message/m_1"), AppModule.CHAT),
            targets.of("home://chat/message/m_1"),
        )
    }

    @Test
    fun aBareChatLinkSelectsTheChatTab() {
        assertEquals(NotificationTarget(Route.Tab("chat"), AppModule.CHAT), targets.of("chat"))
    }

    @Test
    fun alarmReminderAndTimerLinksOpenTheTimeSection() {
        listOf("alarms", "timers", "reminders").forEach { section ->
            assertEquals(
                NotificationTarget(Route.Tab(section), AppModule.ALARMS_TIMERS),
                targets.of("home://$section"),
            )
            assertEquals(
                NotificationTarget(Route.Tab(section), AppModule.ALARMS_TIMERS),
                targets.of("time/$section"),
            )
        }
    }

    @Test
    fun aSecretsLinkOpensTheCatalogForTheOwner() {
        assertEquals(
            NotificationTarget(Route.Link("secrets/catalog"), AppModule.SECRETS, ownerOnly = true),
            targets.of("home://secrets"),
        )
    }

    @Test
    fun aSecretsPathLinkOpensThatPath() {
        val link = "home://secrets/path/infra%2Fbackup%2Fha"
        assertEquals(
            NotificationTarget(Route.Link("secrets/path/infra%2Fbackup%2Fha"), AppModule.SECRETS, ownerOnly = true),
            targets.of(link),
        )
    }

    @Test
    fun linksWithoutAKnownRouteHaveNoTarget() {
        assertNull(targets.of(null))
        assertNull(targets.of(""))
        assertNull(targets.of("https://example.test/chat"))
        assertNull(targets.of("unknown/screen"))
        assertNull(targets.of("access/tokens"))
    }

    @Test
    fun aTokenWithoutTheModuleCannotOpenTheTarget() {
        val chat = requireNotNull(targets.of("chat"))

        assertNull(chat.routeWhenAllowed(access()))
        assertEquals(Route.Tab("chat"), chat.routeWhenAllowed(access(AppModule.CHAT)))
    }

    @Test
    fun onlyAnOwnerWithTheSecretsModuleReachesSecrets() {
        val secrets = requireNotNull(targets.of("home://secrets"))

        assertNull(secrets.routeWhenAllowed(access(AppModule.SECRETS)))
        assertNull(secrets.routeWhenAllowed(access(owner = true)))
        assertEquals(
            Route.Link("secrets/catalog"),
            secrets.routeWhenAllowed(access(AppModule.SECRETS, owner = true)),
        )
    }

    @Test
    fun anUnknownTokenAllowsTheModulesItHasNotDeniedYet() {
        assertEquals(Route.Tab("alarms"), targets.of("alarms")?.routeWhenAllowed(AppAccessState.Unknown))
        assertNull(targets.of("home://secrets")?.routeWhenAllowed(AppAccessState.Unknown))
    }

    private fun timeFeature(): TestEntry = TestEntry(
        id = "time",
        isTab = true,
        module = AppModule.ALARMS_TIMERS,
        rootDestination = TimeCodec.destination(TimeSection.ALARMS),
        decoders = listOf(TimeCodec),
        hosted = TimeSection.entries.associate { it.name.lowercase() to TimeCodec.destination(it) },
    )

    private fun chatFeature(): TestEntry = TestEntry(
        id = "chat",
        isTab = true,
        module = AppModule.CHAT,
        rootDestination = ChatCodec.chat,
        decoders = listOf(ChatCodec),
    )

    private fun secretsFeature(): TestEntry = TestEntry(
        id = "secrets",
        module = AppModule.SECRETS,
        ownerOnly = true,
        rootDestination = SecretsCodec.catalog,
        decoders = listOf(RouteCodec.fixed(SecretsCodec.catalog), SecretsCodec),
    )

    private fun access(vararg modules: AppModule, owner: Boolean = false): AppAccessState =
        AppAccessState.Ready(AppAccess(name = "test", modules = modules.toSet(), owner = owner))
}

internal data class TestDestination(
    override val featureId: String,
    override val route: String,
) : Destination

internal open class TestEntry(
    override val id: String,
    override val isTab: Boolean = false,
    override val module: AppModule = AppModule.BASE,
    override val ownerOnly: Boolean = false,
    override val rootDestination: Destination,
    private val decoders: List<RouteCodec<*>> = listOf(RouteCodec.fixed(rootDestination)),
    private val hosted: Map<String, Destination> = emptyMap(),
    override val notificationLink: Boolean = true,
) : FeatureEntry {
    override val order: Int = 0
    override val titleRes: Int = 0
    override val iconRes: Int = 0
    override val routes: List<RouteCodec<*>> get() = decoders

    override fun createFragment(destination: Destination): Fragment = Fragment()

    override fun hostedDestination(featureId: String): Destination? = hosted[featureId]
}

private enum class TimeSection { ALARMS, TIMERS, REMINDERS }

private object TimeCodec : RouteCodec<TestDestination> {
    fun destination(section: TimeSection): TestDestination =
        TestDestination("time", "time/${section.name.lowercase()}")

    override fun decode(path: RoutePath): TestDestination? =
        path.tail("time")?.let { value ->
            TimeSection.entries.firstOrNull { it.name.equals(value, ignoreCase = true) }?.let(::destination)
        }
}

private object ChatCodec : RouteCodec<TestDestination> {
    val chat = TestDestination("chat", "chat")

    override fun decode(path: RoutePath): TestDestination? =
        chat.takeIf { path.value == chat.route || path.value.startsWith("${chat.route}/") }
}

private object SecretsCodec : RouteCodec<TestDestination> {
    val catalog = TestDestination("secrets", "secrets/catalog")

    override fun decode(path: RoutePath): TestDestination? =
        path.tail("secrets", "path")?.let { TestDestination("secrets", "secrets/path/$it") }
}
