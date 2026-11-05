package wakegrant

import (
	"context"
	"io"
	"log/slog"
	"net/http"
	"path/filepath"
	"testing"
	"time"

	"ricardo.local/monorepo/packages/go/contracttest"
)

const contractToken = "contract-issuer-token"

// contractSecondNonce is a second valid nonce for the bad-signature step.
const contractSecondNonce = "ICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj8="

// TestContractFixtures replays the wakegrant HTTP protocol fixtures. The
// runner owns the transcript; this file wires the handler and seeds the
// published PROTOCOL.md test vector so the vector scenarios pin real MACs.
func TestContractFixtures(t *testing.T) {
	contracttest.Run(t, "solutions/wakegrant/internal/wakegrant/testdata/contract", newContractServer,
		contracttest.WithNormalizer(normalizeWakegrant))
}

func newContractServer(t *testing.T, scenario string) http.Handler {
	t.Helper()
	now := time.Date(2026, 11, 3, 12, 0, 0, 0, time.UTC)
	store, err := OpenStore(context.Background(), filepath.Join(t.TempDir(), "contract.db"))
	if err != nil {
		t.Fatalf("open store: %v", err)
	}
	t.Cleanup(func() { _ = store.Close() })
	svc := New(Options{
		Keys:        Keys{"komp": vectorKey(), "desk": testKey(7)},
		IssuerToken: contractToken,
		Store:       store,
		MaxTTL:      MaxTTL,
		Logger:      slog.New(slog.NewTextHandler(io.Discard, nil)),
		Now:         func() time.Time { return now },
	})
	seedVector(t, svc, store, now, scenario != "claim_denied_vector")
	return svc.Handler()
}

// normalizeWakegrant keeps the pinned PROTOCOL.md vector verbatim and uses
// placeholders only for the scenario that exercises the random generators.
func normalizeWakegrant(scenario string, value any) any {
	if scenario != "grant_and_challenge" {
		return value
	}
	value = contracttest.ReplacePattern(value, contracttest.UUIDPattern, "<uuid>")
	return contracttest.ReplacePattern(value, contracttest.Base64_32Pattern, "<base64-32>")
}

// seedVector pins the PROTOCOL.md test vector: a valid nonce, an unconsumed
// logon grant with the fixed id, and a second valid nonce for the
// bad-signature scenario.
func seedVector(t *testing.T, svc *Service, store *Store, now time.Time, withGrant bool) {
	t.Helper()
	svc.nonces.mu.Lock()
	svc.nonces.entries[vectorNonce] = &nonceEntry{machine: "komp", expires: now.Add(NonceTTL)}
	svc.nonces.entries[contractSecondNonce] = &nonceEntry{machine: "komp", expires: now.Add(NonceTTL)}
	svc.nonces.mu.Unlock()
	if !withGrant {
		return
	}
	if _, err := store.db.ExecContext(context.Background(),
		`INSERT INTO grants(id, machine, purpose, created_unix, expires_unix, consumed) VALUES (?,?,?,?,?,0)`,
		vectorGrantID, "komp", string(PurposeLogon), now.UnixNano(), now.Add(DefaultTTL).Unix()); err != nil {
		t.Fatalf("seed grant: %v", err)
	}
}
