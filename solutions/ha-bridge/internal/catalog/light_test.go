package catalog

import "testing"

func TestNewLightCapabilities(t *testing.T) {
	if NewLight(LightInput{ColorModes: []string{"onoff"}, Brightness: -1}) != nil {
		t.Error("on/off light must not be adjustable")
	}
	ct := NewLight(LightInput{ColorModes: []string{"color_temp"}, MinKelvin: 2700, MaxKelvin: 6500, Brightness: 255, Kelvin: 4000})
	if ct == nil || !ct.Brightness || !ct.ColorTemp || ct.Color || ct.BrightnessPct != 100 || ct.Kelvin != 4000 {
		t.Errorf("color-temp-only light = %+v", ct)
	}
	rgb := NewLight(LightInput{ColorModes: []string{"color_temp", "xy"}, Brightness: 128, Hue: 359.6, Saturation: 50, HasHS: true})
	if rgb == nil || !rgb.Color || !rgb.ColorTemp || rgb.MinKelvin != 2000 || rgb.MaxKelvin != 6500 || rgb.BrightnessPct != 50 || rgb.Hue != 0 {
		t.Errorf("color light = %+v", rgb)
	}
	dim := NewLight(LightInput{ColorModes: []string{"brightness"}, Brightness: -1})
	if dim == nil || dim.ColorTemp || dim.Color || dim.BrightnessPct != 0 {
		t.Errorf("dimmable light = %+v", dim)
	}
}

func TestSetValidation(t *testing.T) {
	ct := &Light{Brightness: true, ColorTemp: true, MinKelvin: 2700, MaxKelvin: 6500}
	i := func(v int) *int { return &v }
	bad := []Params{
		{}, {BrightnessPct: i(0)}, {BrightnessPct: i(101)}, {Kelvin: i(2000)}, {Kelvin: i(7000)},
		{Hue: i(1), Saturation: i(1)}, {Kelvin: i(3000), Hue: i(1), Saturation: i(1)},
	}
	for _, p := range bad {
		if _, err := ct.serviceData(p); err == nil {
			t.Errorf("%+v must be rejected", p)
		}
	}
	color := &Light{Brightness: true, Color: true}
	if _, err := color.serviceData(Params{Hue: i(10)}); err == nil {
		t.Error("hue without saturation must be rejected")
	}
	data, err := color.serviceData(Params{Hue: i(10), Saturation: i(80), BrightnessPct: i(5)})
	if err != nil || data["brightness_pct"] != 5 {
		t.Errorf("data=%v err=%v", data, err)
	}
}
