package dev.proflyder.home.core.model

/**
 * What kind of thing a catalog entry controls. Unknown server kinds map to [OTHER] and are not shown.
 *
 * [PROFILE] is the assistant's named profile (personal/work). It is one global
 * mode rather than content: the Home screen pins it above the scenarios, keeps
 * it out of the device filters and drives it with turn_on ("work") / turn_off
 * ("personal").
 */
enum class ControlKind { SCENARIO, LIGHT, SWITCH, PROFILE, ACTION, OTHER }

/** Commands a catalog entry accepts; [wire] is the value sent to the server. */
enum class ControlAction(val wire: String) {
    RUN("run"),
    TURN_ON("turn_on"),
    TURN_OFF("turn_off"),
    SET("set"),

    /** Speak a text on the speaker; the payload carries `text`. */
    SPEAK("speak"),
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
    /**
     * Device this entry belongs to; empty when the server does not group
     * entries (an older server), in which case the entry is its own device.
     */
    val deviceId: String = "",
    /** Device name shown as the group title; empty when the entry has no device. */
    val device: String = "",
    /** Primary control or a secondary setting of [deviceId]. */
    val role: ControlRole = ControlRole.PRIMARY,
) {
    val isDevice: Boolean get() = kind == ControlKind.LIGHT || kind == ControlKind.SWITCH

    /** The assistant profile selector: a global mode, not a device. */
    val isProfile: Boolean get() = kind == ControlKind.PROFILE
}

data class ControlCatalog(
    val entries: List<ControlEntry>,
    /** False when the server could not read Home Assistant; entries carry no live state then. */
    val statesOk: Boolean,
)
