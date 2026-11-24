package dev.proflyder.home.feature.settings.presentation

import androidx.fragment.app.Fragment
import dev.proflyder.home.core.access.AppAccessSource
import dev.proflyder.home.core.access.AppAccessState
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.core.navigation.FeatureRegistry
import dev.proflyder.home.core.navigation.NotificationTargets
import dev.proflyder.home.core.navigation.RouteCodec
import dev.proflyder.home.core.navigation.RoutePath
import kotlinx.coroutines.flow.MutableStateFlow

internal fun testTargets(): NotificationTargets = NotificationTargets(
    FeatureRegistry(
        listOf(
            TestFeature(id = "time", isTab = true, module = AppModule.ALARMS_TIMERS, rootDestination = timeDestination("alarms"), codecs = listOf(TimeFeatureCodec), hosted = timeSections()),
            TestFeature(id = "alarms", module = AppModule.ALARMS_TIMERS, rootDestination = TestFeatureDestination("alarms", "alarms/list")),
            TestFeature(id = "timers", module = AppModule.ALARMS_TIMERS, rootDestination = TestFeatureDestination("timers", "timers/list")),
            TestFeature(id = "reminders", module = AppModule.ALARMS_TIMERS, rootDestination = TestFeatureDestination("reminders", "reminders/list")),
            TestFeature(id = "chat", isTab = true, module = AppModule.CHAT, rootDestination = TestFeatureDestination("chat", "chat"), codecs = listOf(ChatFeatureCodec)),
            TestFeature(
                id = "secrets",
                module = AppModule.SECRETS,
                ownerOnly = true,
                rootDestination = TestFeatureDestination("secrets", "secrets/catalog"),
                codecs = listOf(RouteCodec.fixed(TestFeatureDestination("secrets", "secrets/catalog")), SecretsFeatureCodec),
            ),
        ),
    ),
)

internal class FakeAccessSource(initial: AppAccessState = AppAccessState.Unknown) : AppAccessSource {
    override val state = MutableStateFlow(initial)
}

private fun timeSections(): Map<String, Destination> =
    listOf("alarms", "timers", "reminders").associateWith(::timeDestination)

private fun timeDestination(section: String): TestFeatureDestination =
    TestFeatureDestination("time", "time/$section")

private data class TestFeatureDestination(
    override val featureId: String,
    override val route: String,
) : Destination

private class TestFeature(
    override val id: String,
    override val isTab: Boolean = false,
    override val module: AppModule = AppModule.BASE,
    override val ownerOnly: Boolean = false,
    override val rootDestination: Destination,
    private val codecs: List<RouteCodec<*>> = listOf(RouteCodec.fixed(rootDestination)),
    private val hosted: Map<String, Destination> = emptyMap(),
) : FeatureEntry {
    override val order: Int = 0
    override val titleRes: Int = 0
    override val iconRes: Int = 0
    override val routes: List<RouteCodec<*>> get() = codecs

    override fun createFragment(destination: Destination): Fragment = Fragment()

    override fun hostedDestination(featureId: String): Destination? = hosted[featureId]
}

private object TimeFeatureCodec : RouteCodec<TestFeatureDestination> {
    override fun decode(path: RoutePath): TestFeatureDestination? =
        path.tail("time")?.let(::timeDestination)
}

private object ChatFeatureCodec : RouteCodec<TestFeatureDestination> {
    override fun decode(path: RoutePath): TestFeatureDestination? =
        TestFeatureDestination("chat", "chat").takeIf { path.value == "chat" || path.value.startsWith("chat/") }
}

private object SecretsFeatureCodec : RouteCodec<TestFeatureDestination> {
    override fun decode(path: RoutePath): TestFeatureDestination? =
        path.tail("secrets", "path")?.let { TestFeatureDestination("secrets", "secrets/path/$it") }
}
