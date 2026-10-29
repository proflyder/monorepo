package catalog

import (
	"errors"
	"fmt"
	"math"
	"strings"
)

// Light describes what a light can be adjusted to, derived from the Home
// Assistant color modes it reports (nothing is listed by hand), plus its live
// values. A bulb with only color temperature has no Color; a dimmable white
// bulb has Brightness alone.
type Light struct {
	Brightness bool
	ColorTemp  bool
	Color      bool
	// MinKelvin/MaxKelvin bound color temperature (set when ColorTemp).
	MinKelvin int
	MaxKelvin int

	// Live values; zero means unknown / not applicable.
	BrightnessPct int
	Kelvin        int
	Hue           int
	Saturation    int
	// Mode is the active mode: "color_temp", "color" or "" (unknown / white-only).
	// HA reports values only for the active mode, and derives the other
	// (a color_temp lamp still shows an hs_color), so inactive values are dropped.
	Mode string
}

// Default white range when Home Assistant does not report one.
const (
	defaultMinKelvin = 2000
	defaultMaxKelvin = 6500
)

// LightInput is the raw Home Assistant data NewLight derives from.
type LightInput struct {
	ColorModes []string
	MinKelvin  int
	MaxKelvin  int
	// Brightness is HA's 0-255 value, -1 when absent.
	Brightness int
	Kelvin     int
	// HS is hue (0-360) and saturation (0-100); HasHS is false when absent.
	Hue, Saturation float64
	HasHS           bool
	// ColorMode is HA's active color_mode ("" when absent).
	ColorMode string
}

// NewLight derives the capabilities of a light, or nil when it is on/off only.
func NewLight(in LightInput) *Light {
	l := &Light{}
	for _, mode := range in.ColorModes {
		switch strings.ToLower(mode) {
		case "brightness", "white":
			l.Brightness = true
		case "color_temp":
			l.Brightness, l.ColorTemp = true, true
		case "hs", "xy", "rgb", "rgbw", "rgbww":
			l.Brightness, l.Color = true, true
		}
	}
	if !l.Brightness {
		return nil
	}
	if l.ColorTemp {
		l.MinKelvin, l.MaxKelvin = in.MinKelvin, in.MaxKelvin
		if l.MinKelvin <= 0 || l.MaxKelvin <= l.MinKelvin {
			l.MinKelvin, l.MaxKelvin = defaultMinKelvin, defaultMaxKelvin
		}
		if l.Mode != "color" && in.Kelvin >= l.MinKelvin && in.Kelvin <= l.MaxKelvin {
			l.Kelvin = in.Kelvin
		}
	}
	switch strings.ToLower(in.ColorMode) {
	case "color_temp":
		l.Mode = "color_temp"
	case "hs", "xy", "rgb", "rgbw", "rgbww":
		l.Mode = "color"
	}
	if in.Brightness >= 0 {
		l.BrightnessPct = int(math.Round(float64(in.Brightness) * 100 / 255))
	}
	if l.Color && in.HasHS && l.Mode != "color_temp" {
		l.Hue = int(math.Round(in.Hue)) % 360
		l.Saturation = int(math.Round(in.Saturation))
	}
	return l
}

// Params are the adjustments of a "set" command. Nil fields are untouched.
type Params struct {
	BrightnessPct *int `json:"brightness,omitempty"`
	Kelvin        *int `json:"kelvin,omitempty"`
	Hue           *int `json:"hue,omitempty"`
	Saturation    *int `json:"saturation,omitempty"`
}

// serviceData validates p against the capabilities and builds light.turn_on data.
func (l *Light) serviceData(p Params) (map[string]any, error) {
	data := map[string]any{}
	if p.BrightnessPct != nil {
		if !l.Brightness {
			return nil, errors.New("light does not support brightness")
		}
		if *p.BrightnessPct < 1 || *p.BrightnessPct > 100 {
			return nil, fmt.Errorf("brightness %d out of range 1-100", *p.BrightnessPct)
		}
		data["brightness_pct"] = *p.BrightnessPct
	}
	hasColor := p.Hue != nil || p.Saturation != nil
	if p.Kelvin != nil && hasColor {
		return nil, errors.New("color temperature and color cannot be set together")
	}
	if p.Kelvin != nil {
		if !l.ColorTemp {
			return nil, errors.New("light does not support color temperature")
		}
		if *p.Kelvin < l.MinKelvin || *p.Kelvin > l.MaxKelvin {
			return nil, fmt.Errorf("color temperature %d out of range %d-%d", *p.Kelvin, l.MinKelvin, l.MaxKelvin)
		}
		data["color_temp_kelvin"] = *p.Kelvin
	}
	if hasColor {
		if !l.Color {
			return nil, errors.New("light does not support color")
		}
		if p.Hue == nil || p.Saturation == nil {
			return nil, errors.New("color needs both hue and saturation")
		}
		if *p.Hue < 0 || *p.Hue > 360 || *p.Saturation < 0 || *p.Saturation > 100 {
			return nil, errors.New("hue must be 0-360 and saturation 0-100")
		}
		data["hs_color"] = []int{*p.Hue, *p.Saturation}
	}
	if len(data) == 0 {
		return nil, errors.New("set needs at least one adjustment")
	}
	return data, nil
}
