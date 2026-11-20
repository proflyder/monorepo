package dev.proflyder.home.feature.system.presentation

import androidx.annotation.StringRes
import dev.proflyder.home.feature.system.impl.R

enum class SystemSection(val route: String, @param:StringRes val titleRes: Int) {
    HOST("host", R.string.system_card_host),
    GUESTS("guests", R.string.system_card_guests),
    CLUSTER("cluster", R.string.system_card_cluster),
    DEVICES("devices", R.string.system_card_devices),
    ;

    companion object {
        fun fromRoute(route: String?): SystemSection = entries.firstOrNull { it.route == route } ?: HOST
    }
}
