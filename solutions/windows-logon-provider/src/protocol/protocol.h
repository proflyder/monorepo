// wakegrant v1 client-side protocol logic.
//
// Portable: no Windows headers, no sockets, no registry. The logon provider and
// the host-side unit tests compile this same file, so the wire format is
// exercised on macOS and used unchanged on Windows.
//
// The contract lives in packages/contracts/wakegrant/PROTOCOL.md.

#pragma once

#include <cstddef>
#include <cstdint>
#include <string>
#include <string_view>
#include <vector>

namespace ricardo {
namespace wakegrant {

using Bytes = std::vector<std::uint8_t>;

// --- encoding -----------------------------------------------------------------

// Standard base64 with padding.
std::string b64_encode(const Bytes& data);

// Strict decoder: rejects non-canonical padding, embedded whitespace and any
// character outside the alphabet. Returns false on any problem.
bool b64_decode(std::string_view text, Bytes* out);

// Length-independent comparison; both inputs are compared in full.
bool constant_time_equal(const Bytes& a, const Bytes& b);
bool constant_time_equal(std::string_view a, std::string_view b);

// --- authentication -----------------------------------------------------------

// HMAC-SHA256 over the UTF-8 bytes of `message`, output base64.
// Windows links the BCrypt implementation; the portable tests link
// hmac_sha256_reference.cpp instead.
std::string hmac_sha256_b64(const Bytes& key, std::string_view message);

// Plain-C++ reference implementation. Never linked into the Windows DLL.
std::string hmac_sha256_b64_reference(const Bytes& key, std::string_view message);

// `<kind>\n<machine>\n<nonce>` and
// `<kind>\n<machine>\n<nonce>\n<granted 0|1>\n<grantId>`.
std::string claim_mac_message(std::string_view machine, std::string_view nonce);
std::string grant_mac_message(std::string_view machine,
                              std::string_view nonce,
                              bool granted,
                              std::string_view grant_id);

// --- requests -----------------------------------------------------------------

std::string challenge_request_json(std::string_view machine);
std::string claim_request_json(std::string_view machine, std::string_view nonce);

// --- responses ----------------------------------------------------------------

struct ChallengeResponse {
    std::string nonce;       // base64, validated to decode to 32 bytes
    long long expires_in = 0;  // seconds, 0 when absent
};

// Returns false (and fills `error`) on malformed or implausible responses.
bool parse_challenge_response(std::string_view body, ChallengeResponse* out, std::string* error);

enum class ClaimOutcome {
    Granted,     // granted:true and a valid response mac
    Denied,      // granted:false and a valid response mac
    InvalidMac,  // missing, undecodable or wrong mac: treated as denied
    Malformed,   // not a well-formed claim response at all
};

struct ClaimResult {
    ClaimOutcome outcome = ClaimOutcome::Malformed;
    std::string grant_id;
    std::string detail;  // short, log-safe reason (never contains secrets)
};

// Verifies the server response for `nonce` with `key`. Only Granted may arm the
// provider; every other outcome is fail-closed.
ClaimResult verify_claim_response(std::string_view body,
                                  const Bytes& key,
                                  std::string_view machine,
                                  std::string_view nonce);

// --- configuration ------------------------------------------------------------

struct ServiceEndpoint {
    bool tls = false;
    std::string host;   // DNS name, IPv4 or IPv6 literal without brackets
    std::uint16_t port = 80;
    std::string prefix;  // "" or "/path" without trailing slash
};

// Parses `http://host[:port][/path]` or `https://...`. Rejects userinfo,
// queries, fragments, whitespace and control characters.
bool parse_service_url(std::string_view url, ServiceEndpoint* out, std::string* error);

// Validates the whole machine configuration and normalizes the URL. Returns
// false and sets `error` (log-safe) when anything is off.
bool validate_config(std::string_view service_url,
                     std::string_view machine,
                     std::string_view user,
                     std::string_view domain,
                     std::string* normalized_url,
                     std::string* error);

// Log-safe service URL for log lines: scheme, host and port only, no path.
std::string redact_service_url(std::string_view url);

// Request target path for an endpoint, e.g. "/v1/claim" or "/gw/v1/claim".
std::string endpoint_path(const ServiceEndpoint& endpoint, std::string_view path);

// --- secret file --------------------------------------------------------------

// Plaintext payload of C:\ProgramData\Ricardo\KompLogon\secret.bin after DPAPI
// machine-scope decryption:
//   {"version":1,"password":"<utf-8>","key":"<base64 of 32 bytes>"}
struct MachineSecret {
    std::string password;
    Bytes key;
};

bool parse_secret_json(std::string_view body, MachineSecret* out, std::string* error);

}  // namespace wakegrant
}  // namespace ricardo
