package dev.proflyder.home.feature.secrets

import android.net.Uri
import androidx.fragment.app.Fragment
import dev.proflyder.home.core.model.AppModule
import dev.proflyder.home.core.navigation.Destination
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.core.navigation.RouteCodec
import dev.proflyder.home.core.navigation.RoutePath
import dev.proflyder.home.feature.secrets.impl.R
import dev.proflyder.home.feature.secrets.presentation.SecretPathFragment
import dev.proflyder.home.feature.secrets.presentation.SecretsCatalogFragment
import dev.proflyder.home.feature.secrets.presentation.SecretsDeviceFragment
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject

internal object SecretsCatalogDestination : Destination {
    override val featureId: String = SecretsRoutes.FEATURE_ID
    override val route: String = SecretsRoutes.CATALOG
}

internal object SecretsDeviceDestination : Destination {
    override val featureId: String = SecretsRoutes.FEATURE_ID
    override val route: String = SecretsRoutes.DEVICE
}

internal data class SecretsPathDestination(val path: String) : Destination {
    override val featureId: String = SecretsRoutes.FEATURE_ID
    override val route: String = "${SecretsRoutes.PATH_PREFIX}/${Uri.encode(path)}"

    companion object : RouteCodec<SecretsPathDestination> {
        override fun decode(path: RoutePath): SecretsPathDestination? =
            path.tail(SecretsRoutes.FEATURE_ID, "path")
                ?.let(Uri::decode)
                ?.takeIf(String::isNotBlank)
                ?.let(::SecretsPathDestination)
    }
}

@ContributesIntoSet(AppScope::class)
@Inject
class SecretsFeatureEntry : FeatureEntry {
    override val id: String = SecretsRoutes.FEATURE_ID
    override val order: Int = 21
    override val isTab: Boolean = false
    override val module: AppModule = AppModule.SECRETS
    override val ownerOnly: Boolean = true
    override val titleRes: Int = R.string.secrets_title
    override val iconRes: Int = R.drawable.secrets_ic_key
    override val rootDestination: Destination = SecretsCatalogDestination
    override val routes: List<RouteCodec<*>> = listOf(
        RouteCodec.fixed(SecretsCatalogDestination),
        RouteCodec.fixed(SecretsDeviceDestination),
        SecretsPathDestination,
    )

    override fun createFragment(destination: Destination): Fragment = when (destination) {
        is SecretsPathDestination -> SecretPathFragment()
        is SecretsDeviceDestination -> SecretsDeviceFragment()
        else -> SecretsCatalogFragment()
    }
}
