package dev.proflyder.home.feature.access

import androidx.fragment.app.Fragment
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.core.navigation.RouteCodec
import dev.proflyder.home.feature.access.impl.R
import dev.proflyder.home.feature.access.presentation.AccessFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

internal object AccessTokensDestination : Destination {
    override val featureId: String = AccessRoutes.FEATURE_ID
    override val route: String = AccessRoutes.TOKENS
}

@ContributesIntoSet(AppScope::class)
@Inject
class AccessFeatureEntry : FeatureEntry {
    override val id: String = AccessRoutes.FEATURE_ID
    override val order: Int = 20
    override val isTab: Boolean = false
    override val module: AppModule = AppModule.ADMIN
    override val notificationLink: Boolean = false
    override val titleRes: Int = R.string.access_title
    override val iconRes: Int = R.drawable.access_ic_key
    override val rootDestination: Destination = AccessTokensDestination
    override val routes: List<RouteCodec<*>> = listOf(RouteCodec.fixed(AccessTokensDestination))

    override fun createFragment(destination: Destination): Fragment = AccessFragment()
}
