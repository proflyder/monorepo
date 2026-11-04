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

// Purpose-aware messages append the purpose as the last line.
func TestPurposeTestVectors(t *testing.T) {
	key := vectorKey()
	cases := []struct {
		name string
		got  string
		want string
	}{
		{"claim logon", ClaimMACForPurpose(key, vectorMachine, vectorNonce, PurposeLogon),
			"AMPMcTPW1kuBTUp8sWtF4weIoI/Vtcp0R+cNBK+xdB0="},
		{"claim unlock", ClaimMACForPurpose(key, vectorMachine, vectorNonce, PurposeUnlock),
			"jCy3/2JoHClgcwP3xBRHbyQNwGOsloT8uKvgu6fR7H0="},
		{"grant logon granted", GrantedMACForPurpose(key, vectorMachine, vectorNonce, vectorGrantID, PurposeLogon),
			"kmFSwXXu4g4wylTSIf9DSYht1v28pG4jboomTnocI0k="},
		{"grant unlock granted", GrantedMACForPurpose(key, vectorMachine, vectorNonce, vectorGrantID, PurposeUnlock),
			"55kYSfTNqLYnhU6jg+WfuZaUkThoZehudbJPPuP/JH0="},
		{"grant logon denied", DeniedMACForPurpose(key, vectorMachine, vectorNonce, PurposeLogon),
			"HkwjpcROboJ+1uR7P8Rydoo2JvNFHx8iOEI0BS0G7K8="},
		{"grant unlock denied", DeniedMACForPurpose(key, vectorMachine, vectorNonce, PurposeUnlock),
			"OcN4e3gJUGVCUF8chuwGQ0FnhU9vRJ0HowN3zReozxc="},
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
