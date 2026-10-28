// Package idgen produces stable identifiers for scheduler resources and
// events. Resource ids are time-ordered UUIDv7; event ids are UUIDv5 over a
// dedupe identity so a repeated tick, lease redelivery or process restart
// yields the same event id.
package idgen

import (
	"crypto/rand"
	"crypto/sha1"
	"encoding/hex"
	"fmt"
	"time"
)

// dnsNamespace is the RFC 4122 DNS namespace.
var dnsNamespace = [16]byte{
	0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x11, 0xd1,
	0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8,
}

// NewResourceID returns a UUIDv7 for a schedule or timer.
func NewResourceID(now time.Time) (string, error) {
	var b [16]byte
	ms := uint64(now.UnixMilli())
	// 48-bit big-endian unix milliseconds in bytes 0..5.
	b[0] = byte(ms >> 40)
	b[1] = byte(ms >> 32)
	b[2] = byte(ms >> 24)
	b[3] = byte(ms >> 16)
	b[4] = byte(ms >> 8)
	b[5] = byte(ms)
	if _, err := rand.Read(b[6:]); err != nil {
		return "", fmt.Errorf("idgen: read random: %w", err)
	}
	b[6] = (b[6] & 0x0f) | 0x70 // version 7
	b[8] = (b[8] & 0x3f) | 0x80 // RFC 4122 variant
	return format(b), nil
}

// EventID returns the deterministic UUIDv5 for a dedupe identity.
func EventID(dedupeKey string) string {
	h := sha1.New()
	_, _ = h.Write(dnsNamespace[:])
	_, _ = h.Write([]byte(dedupeKey))
	sum := h.Sum(nil)
	var b [16]byte
	copy(b[:], sum[:16])
	b[6] = (b[6] & 0x0f) | 0x50 // version 5
	b[8] = (b[8] & 0x3f) | 0x80 // RFC 4122 variant
	return format(b)
}

func format(b [16]byte) string {
	var out [36]byte
	hex.Encode(out[0:8], b[0:4])
	out[8] = '-'
	hex.Encode(out[9:13], b[4:6])
	out[13] = '-'
	hex.Encode(out[14:18], b[6:8])
	out[18] = '-'
	hex.Encode(out[19:23], b[8:10])
	out[23] = '-'
	hex.Encode(out[24:36], b[10:16])
	return string(out[:])
}

// HashRequest is a stable hex digest of a normalized request used for
// idempotency conflict detection. Callers pass a canonical string; hashing it
// keeps stored rows small and avoids retaining payload bytes.
func HashRequest(canonical string) string {
	sum := sha1.Sum([]byte(canonical))
	return hex.EncodeToString(sum[:])
}
