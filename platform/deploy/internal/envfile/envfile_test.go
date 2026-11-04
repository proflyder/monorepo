package envfile

import (
	"os"
	"path/filepath"
	"testing"
)

func write(t *testing.T, content string) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "deploy.env")
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestParse(t *testing.T) {
	path := write(t, `# a comment
LAN_CIDR=192.0.2.0/24

  NODE_LAN_IP = 192.0.2.27
QUOTED="value with spaces"
SINGLE='single quoted'
export EXPORTED=yes
EMPTY=
URL=https://bot.example.test/telegram/webhook?x=%20
`)
	values, err := Parse(path)
	if err != nil {
		t.Fatalf("parse: %v", err)
	}
	want := map[string]string{
		"LAN_CIDR":    "192.0.2.0/24",
		"NODE_LAN_IP": "192.0.2.27",
		"QUOTED":      "value with spaces",
		"SINGLE":      "single quoted",
		"EXPORTED":    "yes",
		"EMPTY":       "",
		"URL":         "https://bot.example.test/telegram/webhook?x=%20",
	}
	for key, expected := range want {
		if values[key] != expected {
			t.Errorf("%s = %q, want %q", key, values[key], expected)
		}
	}
	if len(values) != len(want) {
		t.Errorf("parsed %d keys, want %d: %v", len(values), len(want), values)
	}
}

func TestParseRejectsMalformedAndDuplicate(t *testing.T) {
	_, err := Parse(write(t, "LAN_CIDR=192.0.2.0/24\nnot a pair\n"))
	if err == nil {
		t.Fatal("a line without `=` must be rejected")
	}
	_, err = Parse(write(t, "LAN_CIDR=a\nLAN_CIDR=b\n"))
	if err == nil {
		t.Fatal("a duplicate key must be rejected")
	}
}

func TestLookup(t *testing.T) {
	path := write(t, "# comment\nEXPORTED=first\nexport EXPORTED=second\nQUOTED=\"a b\"\n")
	value, found, err := Lookup(path, "EXPORTED")
	if err != nil || !found || value != "first" {
		t.Fatalf("lookup: value=%q found=%v err=%v (the first match wins)", value, found, err)
	}
	value, found, err = Lookup(path, "QUOTED")
	if err != nil || !found || value != "a b" {
		t.Fatalf("lookup quoted: value=%q found=%v err=%v", value, found, err)
	}
	value, found, err = Lookup(path, "MISSING")
	if err != nil || found || value != "" {
		t.Fatalf("lookup missing: value=%q found=%v err=%v", value, found, err)
	}
	if _, _, err := Lookup(filepath.Join(t.TempDir(), "absent"), "X"); err == nil {
		t.Fatal("a missing file must be an error")
	}
}

func TestUnquoteMatchesPython(t *testing.T) {
	cases := map[string]string{
		" plain ":    "plain",
		`"quoted"`:   "quoted",
		`'quoted'`:   "quoted",
		`""double""`: "double",
		`"'nested'"`: "nested",
		`"a'b"`:      "a'b",
		"no quotes":  "no quotes",
		"":           "",
	}
	for input, want := range cases {
		if got := Unquote(input); got != want {
			t.Errorf("Unquote(%q) = %q, want %q", input, got, want)
		}
	}
}
