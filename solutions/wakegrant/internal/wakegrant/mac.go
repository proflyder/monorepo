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

// The purpose-aware layout appends the purpose as the last line. It is used
// only when the claim request carries an explicit "purpose"; a v1 client that
// omits the field keeps the messages above byte for byte.

// ClaimMACForPurpose is the machine's proof for a purpose-aware POST /v1/claim.
func ClaimMACForPurpose(key []byte, machine, nonce string, purpose Purpose) string {
	return mac(key, "claim\n"+machine+"\n"+nonce+"\n"+string(purpose))
}

// GrantedMACForPurpose authenticates a successful purpose-aware claim response.
func GrantedMACForPurpose(key []byte, machine, nonce, grantID string, purpose Purpose) string {
	return mac(key, "grant\n"+machine+"\n"+nonce+"\n1\n"+grantID+"\n"+string(purpose))
}

// DeniedMACForPurpose authenticates a denied purpose-aware claim response. The
// empty field before the purpose is the absent grantId, same as v1.
func DeniedMACForPurpose(key []byte, machine, nonce string, purpose Purpose) string {
	return mac(key, "grant\n"+machine+"\n"+nonce+"\n0\n\n"+string(purpose))
}

func mac(key []byte, message string) string {
	h := hmac.New(sha256.New, key)
	h.Write([]byte(message))
	return base64.StdEncoding.EncodeToString(h.Sum(nil))
}
