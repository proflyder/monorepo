package wakegrant

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
)

// The message layout is fixed by packages/contracts/wakegrant/PROTOCOL.md:
// fields are joined with "\n" and there is no trailing newline.

// ClaimMAC is the machine's proof for POST /v1/claim.
func ClaimMAC(key []byte, machine, nonce string) string {
	return mac(key, "claim\n"+machine+"\n"+nonce)
}

// GrantedMAC authenticates a successful claim response.
func GrantedMAC(key []byte, machine, nonce, grantID string) string {
	return mac(key, "grant\n"+machine+"\n"+nonce+"\n1\n"+grantID)
}

// DeniedMAC authenticates a denied claim response.
func DeniedMAC(key []byte, machine, nonce string) string {
	return mac(key, "grant\n"+machine+"\n"+nonce+"\n0\n")
}

func mac(key []byte, message string) string {
	h := hmac.New(sha256.New, key)
	h.Write([]byte(message))
	return base64.StdEncoding.EncodeToString(h.Sum(nil))
}
