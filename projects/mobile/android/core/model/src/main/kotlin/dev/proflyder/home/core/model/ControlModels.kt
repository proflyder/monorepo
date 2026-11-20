package dev.proflyder.home.core.model

enum class ControlKind { SCENARIO, LIGHT, SWITCH, BUTTON, PROFILE, ACTION, VOLUME, OTHER }

enum class ControlAction(val wire: String) {
    RUN("run"),
    TURN_ON("turn_on"),
    TURN_OFF("turn_off"),
    SET("set"),

    PRESS("press"),

    SPEAK("speak"),

    MUTE("mute"),

    UNMUTE("unmute"),
    ;

    companion object {
        fun fromWire(value: String): ControlAction? = entries.firstOrNull { it.wire == value }
    }
}

enum class PowerState { ON, OFF }

enum class ControlRole { PRIMARY, SETTING }

data class LightControls(
    val brightness: Boolean,
    val colorTemp: Boolean,
    val color: Boolean,
    val minKelvin: Int = 0,
    val maxKelvin: Int = 0,
    val brightnessPct: Int = 0,
    val kelvin: Int = 0,
    val hue: Int = 0,
    val saturation: Int = 0,
    val mode: String = "",
    val effects: List<String> = emptyList(),
)

data class LightAdjust(
    val brightnessPct: Int? = null,
    val kelvin: Int? = null,
    val hue: Int? = null,
    val saturation: Int? = null,
)

data class VolumeControls(
    val level: Int = 0,
    val muted: Boolean = false,
)

data class VisibleWhen(
    val entry: String,
    val state: String,
)

data class ControlEntry(
    val id: String,
    val kind: ControlKind,
    val title: String,
    val room: String,
    val icon: String,
    val actions: Set<ControlAction>,
    val power: PowerState?,
    val available: Boolean,
    val light: LightControls? = null,
    val volume: VolumeControls? = null,
    val titleKey: String = "",
    val titleArgs: Map<String, String> = emptyMap(),
    val detailKey: String = "",
    val deviceId: String = "",
    val device: String = "",
    val role: ControlRole = ControlRole.PRIMARY,
    val visibleWhen: VisibleWhen? = null,
    val volumeCompanion: ControlEntry? = null,
) {
    val isDevice: Boolean get() = kind == ControlKind.LIGHT || kind == ControlKind.SWITCH

    val isButton: Boolean get() = kind == ControlKind.BUTTON

    val isProfile: Boolean get() = kind == ControlKind.PROFILE

    val isVolume: Boolean get() = kind == ControlKind.VOLUME

    val displayName: String
        get() {
            val name = title.trim()
            return if (device.isNotBlank() && isTechnicalEntityTitle()) device.trim() else name.ifEmpty { device.trim() }
        }

    val secondaryName: String?
        get() {
            val name = title.trim()
            val deviceName = device.trim()
            if (deviceName.isEmpty() || displayName == deviceName) return null
            return deviceName.takeUnless { it == name }
        }

    fun isTechnicalEntityTitle(): Boolean {
        val name = title.trim().lowercase()
        if (name.isEmpty()) return true
        val deviceName = device.trim().lowercase()
        if (deviceName.isNotEmpty() && name == deviceName) return true
        val domain = id.substringBefore('.').lowercase()
        return name == domain || name == id.lowercase() || name in TECHNICAL_TITLES
    }

    private companion object {
        val TECHNICAL_TITLES = setOf(
            "light",
            "switch",
            "scene",
            "script",
            "action",
            "profile",
            "binary_sensor",
            "sensor",
        )
    }
}

data class ControlCatalog(
    val entries: List<ControlEntry>,
    val statesOk: Boolean,
)
