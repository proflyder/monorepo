package dev.proflyder.home.core.model

/**
 * What kind of thing a catalog entry controls. Unknown server kinds map to [OTHER] and are not shown.
 *
 * [PROFILE] is the assistant's named profile (personal/work). It is one global
 * mode rather than content: the Home screen pins it above the scenarios, keeps
 * it out of the device filters and drives it with turn_on ("work") / turn_off
 * ("personal").
 */
enum class ControlKind { SCENARIO, LIGHT, SWITCH, BUTTON, PROFILE, ACTION, VOLUME, OTHER }

/** Commands a catalog entry accepts; [wire] is the value sent to the server. */
enum class ControlAction(val wire: String) {
    RUN("run"),
    TURN_ON("turn_on"),
    TURN_OFF("turn_off"),
    SET("set"),

    /** Press a stateless Home Assistant button (lock/unlock the PC, ...). */
    PRESS("press"),

    /** Speak a text on the speaker; the payload carries `text`. */
    SPEAK("speak"),

    /** Silence the station volume; no payload. */
    MUTE("mute"),

    /** Restore the station volume; no payload. */
    UNMUTE("unmute"),
    ;

    companion object {
        fun fromWire(value: String): ControlAction? = entries.firstOrNull { it.wire == value }
    }
}

enum class PowerState { ON, OFF }

/**
 * Where an entry sits inside its device: [PRIMARY] entries are the controls
 * shown on the device card, [SETTING] entries are secondary switches hidden
 * behind "Settings (N)". Entries without a device are always primary.
 */
enum class ControlRole { PRIMARY, SETTING }

/**
 * One scenario or device from the home control catalog. [id] is the catalog id
 * (not a Home Assistant entity id); [icon] is a free hint the UI may map to art.
 */
/**
 * What an adjustable light can do and its current values, as reported by the
 * server (derived from Home Assistant color modes). A white-only bulb has
 * [colorTemp] without [color]; unknown live values are 0.
 */
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
    /** Active mode: "color_temp", "color" or empty. Live values are valid for that mode only. */
    val mode: String = "",
    /** `effect_list` reported by Home Assistant; empty when the light has no effects. */
    val effects: List<String> = emptyList(),
)

/** A light adjustment; null fields are left as they are. Color is hue + saturation together. */
data class LightAdjust(
    val brightnessPct: Int? = null,
    val kelvin: Int? = null,
    val hue: Int? = null,
    val saturation: Int? = null,
)

/**
 * Live station volume: [level] is 0–100 and [muted] is the media player mute.
 * Announcement volume reports the number entity and is never muted.
 */
data class VolumeControls(
    val level: Int = 0,
    val muted: Boolean = false,
)

/**
 * A server-side visibility condition: the entry is shown only while
 * [entry] (another catalog id) has the given power state ("on"/"off"). It is
 * additive metadata, not a client rule, so a new conditional control needs no
 * app change.
 */
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
    /** Live on/off for devices; null for scenarios or when the state is unknown. */
    val power: PowerState?,
    val available: Boolean,
    /** Set for lights that can be adjusted beyond on/off. */
    val light: LightControls? = null,
    /** Set for the station volume entries. */
    val volume: VolumeControls? = null,
    /**
     * Stable localization key of a server-generated title (for example the
     * synthetic "Speak on the station" action). Empty when [title] is Home
     * Assistant user data and must be shown verbatim.
     */
    val titleKey: String = "",
    /** Typed arguments for [titleKey]. */
    val titleArgs: Map<String, String> = emptyMap(),
    /**
     * Stable localization key of a device status shown under the title (for
     * example `catalog.status.locked` for the PC). Empty when there is no
     * status; the server never sends display text here.
     */
    val detailKey: String = "",
    /**
     * Device this entry belongs to; empty when the server does not group
     * entries (an older server), in which case the entry is its own device.
     */
    val deviceId: String = "",
    /** Device name shown as the group title; empty when the entry has no device. */
    val device: String = "",
    /** Primary control or a secondary setting of [deviceId]. */
    val role: ControlRole = ControlRole.PRIMARY,
    /** Show this entry only while another catalog entry has a given state. */
    val visibleWhen: VisibleWhen? = null,
) {
    val isDevice: Boolean get() = kind == ControlKind.LIGHT || kind == ControlKind.SWITCH

    /** A stateless one-shot button (lock/unlock); grouped as a device action. */
    val isButton: Boolean get() = kind == ControlKind.BUTTON

    /** The assistant profile selector: a global mode, not a device. */
    val isProfile: Boolean get() = kind == ControlKind.PROFILE

    /** A station volume row: a slider sheet, not an on/off toggle. */
    val isVolume: Boolean get() = kind == ControlKind.VOLUME

    /**
     * Name shown as the main label. Home Assistant often reports only the
     * technical entity name ("Light", "Switch") while the meaningful name is
     * the device ("Лампа", "Светильник"); the device wins in that case.
     */
    val displayName: String
        get() {
            val name = title.trim()
            return if (device.isNotBlank() && isTechnicalEntityTitle()) device.trim() else name.ifEmpty { device.trim() }
        }

    /**
     * Device name for a subtitle when [displayName] is the entry's own title
     * and the device adds information; null when it would duplicate the title
     * or when [displayName] is already the device.
     */
    val secondaryName: String?
        get() {
            val name = title.trim()
            val deviceName = device.trim()
            if (deviceName.isEmpty() || displayName == deviceName) return null
            return deviceName.takeUnless { it == name }
        }

    /** True when the title is an entity id/domain default rather than a human name. */
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
    /** False when the server could not read Home Assistant; entries carry no live state then. */
    val statesOk: Boolean,
)
