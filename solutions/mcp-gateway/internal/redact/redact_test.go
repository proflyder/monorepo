package redact

import (
	"strings"
	"testing"
)

func TestTextRedactsSecrets(t *testing.T) {
	// Split so the scanner does not see a contiguous token in this test file.
	jwt := "jwt eyJhbGciOiJIUzI1NiJ9" + "." + "eyJzdWIiOiIxMjM0NTY3ODkwIn0" + "." + "dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U"
	inputs := []string{
		"Authorization: Bearer abcdefghijklmnopqrstuvwxyz",
		"authorization: token s3cr3t-value",
		"bearer eyJhbGciOiJIUzI1NiJ9" + ".payload.signature",
		"token=abc123secret",
		"password: hunter2hunter2",
		"api_key = \"sk-abcdefghijklmnopqrstuvwx\"",
		"secret: topsecretvalue",
		jwt,
		"key 0123456789abcdef0123456789abcdef",
		"hash 1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d",
		"bot 123456789:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw",
		"blob QWxhZGRpbjpvcGVuIHNlc2FtZQ1234567890abcdef",
	}
	for _, input := range inputs {
		out := Text(input)
		if !strings.Contains(out, Placeholder) {
			t.Errorf("Text(%q) = %q, want %s", input, out, Placeholder)
		}
	}
}

func TestTextKeepsOrdinaryText(t *testing.T) {
	inputs := []string{
		"Планировщик работает, готовы 1 из 1.",
		"2026-11-01T10:00:00Z level=info msg=started",
		"pod scheduler-7d9f8c6b5-x2x4z restarted 1 time",
		"GET /api/v1/query?query=up 200",
		"version 1.8.0",
	}
	for _, input := range inputs {
		if out := Text(input); out != input {
			t.Errorf("Text(%q) = %q, want unchanged", input, out)
		}
	}
}

func TestTextKeepsPrefixForLabeledSecrets(t *testing.T) {
	out := Text("Authorization: Bearer abcdefghijkl")
	if !strings.Contains(out, "Authorization: Bearer") {
		t.Errorf("prefix lost: %q", out)
	}
	out = Text("password=supersecret")
	if !strings.HasPrefix(out, "password=") {
		t.Errorf("key lost: %q", out)
	}
}

// TestEachPatternHidesSecret applies every pattern in isolation. A capture
// group may only hold the non-secret prefix, so the secret text must never
// survive even when the rule is the only one running.
func TestEachPatternHidesSecret(t *testing.T) {
	jwt := "eyJhbGciOiJIUzI1NiJ9" + "." + "eyJzdWIiOiIxMjM0NTY3ODkwIn0" + "." + "S3CRET3SIGNATURE012345"
	cases := []struct {
		name   string
		index  int
		input  string
		secret string
	}{
		{"authorization header", 0, "Authorization: Bearer S3CRET0VALUE", "S3CRET0VALUE"},
		{"bare bearer", 1, "bearer S3CRET1VALUE", "S3CRET1VALUE"},
		{"key value", 2, "password=S3CRET2VALUE", "S3CRET2VALUE"},
		{"jwt", 3, "jwt " + jwt, jwt},
		{"telegram bot", 4, "bot 123456789:S3CRET4TOKEN0123456789abcdefghij", "S3CRET4TOKEN0123456789abcdefghij"},
		{"hex blob", 5, "hash deadbeefdeadbeefdeadbeefdeadbeef", "deadbeefdeadbeefdeadbeefdeadbeef"},
		{"base64 blob", 6, "blob S3CRET6BASE64VALUE0123456789abcdef", "S3CRET6BASE64VALUE0123456789abcdef"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			out := redactWith(patterns[tc.index], tc.input)
			if strings.Contains(out, tc.secret) {
				t.Fatalf("pattern %d leaked the secret: %q", tc.index, out)
			}
			if !strings.Contains(out, Placeholder) {
				t.Fatalf("pattern %d did not redact: %q", tc.index, out)
			}
		})
	}
}
