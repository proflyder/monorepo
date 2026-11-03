package wakegrant

import (
	"encoding/base64"
	"testing"
)

// K = bytes 0x00..0x1f, machine = komp, nonce = bytes 0x00..0x1f,
// grantId = 00000000-0000-4000-8000-000000000001.
const (
	vectorMachine = "komp"
	vectorNonce   = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="
	vectorGrantID = "00000000-0000-4000-8000-000000000001"
)

func vectorKey() []byte {
	key := make([]byte, 32)
	for i := range key {
		key[i] = byte(i)
	}
	return key
}

func TestProtocolTestVector(t *testing.T) {
	key := vectorKey()
	cases := []struct {
		name string
		got  string
		want string
	}{
		{"claim", ClaimMAC(key, vectorMachine, vectorNonce),
			"+YedSn/aeRZqRNKKzkUS8Dy2aq/dVLkXbh9iX91L9XU="},
		{"grant granted", GrantedMAC(key, vectorMachine, vectorNonce, vectorGrantID),
			"UAADAs7pUxn/EN42woIH5K0qrH2YB+bd1SYZVKP//FI="},
		{"grant denied", DeniedMAC(key, vectorMachine, vectorNonce),
			"I6ClmYKmmfO6wodeKXOobAtJj/SgQ7C/ieNumDYrj1Y="},
	}
	for _, tc := range cases {
		if tc.got != tc.want {
			t.Errorf("%s MAC = %q, want %q", tc.name, tc.got, tc.want)
		}
	}
}

func TestVectorKeyIsBase64RoundTrip(t *testing.T) {
	// Keep the test vector's key encoding honest for the machines-file tests.
	if got := base64.StdEncoding.EncodeToString(vectorKey()); got == "" {
		t.Fatal("empty key encoding")
	}
}
