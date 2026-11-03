// Package buildid computes the short deploy build identifier.
//
// The deploy scripts used to inline a one-line Python expression:
//
//	BUILD_ID="$(git rev-parse --short HEAD)-$(printf '%s' "$FINGERPRINT" \
//	  | python3 -c 'import hashlib,sys; print(hashlib.sha256(sys.stdin.read().encode()).hexdigest()[:12])')"
//
// The git half stays in the shell (it needs the checkout); this package owns
// the fingerprint half so every unit derives the same id from the same image
// set.
package buildid

import (
	"crypto/sha256"
	"encoding/hex"
)

// ShortLength is the number of hex characters kept from the digest.
const ShortLength = 12

// Fingerprint returns the first 12 hex characters of SHA-256(fingerprint).
func Fingerprint(fingerprint string) string {
	digest := sha256.Sum256([]byte(fingerprint))
	return hex.EncodeToString(digest[:])[:ShortLength]
}

// BuildID joins the short git revision and the fingerprint digest the way the
// deploy scripts always have: `<rev>-<12 hex chars>`.
func BuildID(revision, fingerprint string) string {
	return revision + "-" + Fingerprint(fingerprint)
}
