package wakegrant

import (
	"encoding/base64"
	"strings"
	"testing"
)

func testKey(fill byte) []byte {
	key := make([]byte, KeySize)
	for i := range key {
		key[i] = fill
	}
	return key
}

func TestParseMachinesJSON(t *testing.T) {
	data := []byte(`{"komp":"` + base64.StdEncoding.EncodeToString(testKey(1)) + `"}`)
	keys, err := ParseMachines(data)
	if err != nil {
		t.Fatalf("ParseMachines: %v", err)
	}
	if len(keys) != 1 || len(keys["komp"]) != KeySize {
		t.Fatalf("unexpected keys: %#v", keys)
	}
}

func TestParseMachinesLines(t *testing.T) {
	encoded := base64.StdEncoding.EncodeToString(testKey(2))
	for _, body := range []string{
		"# machines\nkomp=" + encoded + "\n",
		"komp " + encoded + "\n",
	} {
		keys, err := ParseMachines([]byte(body))
		if err != nil {
			t.Fatalf("ParseMachines(%q): %v", body, err)
		}
		if len(keys["komp"]) != KeySize {
			t.Fatalf("unexpected key: %#v", keys)
		}
	}
}

func TestParseMachinesRejectsBadInput(t *testing.T) {
	short := base64.StdEncoding.EncodeToString(testKey(3)[:16])
	cases := map[string]string{
		"empty":        "",
		"bad json":     `{"komp":`,
		"bad machine":  `{"KOMP!":"` + base64.StdEncoding.EncodeToString(testKey(4)) + `"}`,
		"short key":    "komp=" + short,
		"bad base64":   "komp=not-base64!!",
		"missing key":  "komp=",
		"duplicate":    "komp=" + base64.StdEncoding.EncodeToString(testKey(5)) + "\nkomp=" + base64.StdEncoding.EncodeToString(testKey(6)),
		"no separator": "komp",
		"too long id":  strings.Repeat("a", 33) + "=" + base64.StdEncoding.EncodeToString(testKey(7)),
	}
	for name, body := range cases {
		if _, err := ParseMachines([]byte(body)); err == nil {
			t.Errorf("%s: expected an error", name)
		}
	}
}

func TestKeysNamesAreSortedAndDoNotExposeKeys(t *testing.T) {
	keys := Keys{
		"b": testKey(1),
		"a": testKey(2),
	}
	names := keys.Names()
	if len(names) != 2 || names[0] != "a" || names[1] != "b" {
		t.Fatalf("Names() = %v", names)
	}
}
