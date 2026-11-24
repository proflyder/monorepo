package dev.proflyder.home.core.model

enum class AppModule(val wire: String) {
    BASE("base"),
    HOME("home"),
    SCENARIOS("scenarios"),
    ALARMS_TIMERS("alarms_timers"),
    INTERCOM_CALL("intercom_call"),
    NOTIFICATIONS("notifications"),
    CHAT("chat"),
    VPN("vpn"),
    SYSTEM("system"),
    INFRA_REPORTS("infra_reports"),
    ALERTS("alerts"),
    SECRETS("secrets"),
    ADMIN("admin"),
    ;

    companion object {
        fun fromWire(raw: String): AppModule? = entries.firstOrNull { it.wire == raw.trim() }

        fun fromWire(values: List<String>): Set<AppModule> = values.mapNotNull(::fromWire).toSet()
    }
}

data class AppAccess(
    val name: String,
    val modules: Set<AppModule>,
    val owner: Boolean,
) {
    val administrator: Boolean get() = AppModule.ADMIN in modules

    fun allows(module: AppModule): Boolean = module in modules
}

data class AccessToken(
    val id: String,
    val name: String,
    val modules: Set<AppModule>,
    val owner: Boolean,
    val createdUnix: Long,
    val updatedUnix: Long,
) : Keyed {
    override val key: String get() = id
}

data class CreatedAccessToken(val token: String, val access: AccessToken)
