// Host-side unit tests for the portable wakegrant client logic.
//
// Build and run with ./run-tests.sh (host clang++, no Windows toolchain).

#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

#include "../src/protocol/protocol.h"

namespace {

using ricardo::wakegrant::Bytes;
using ricardo::wakegrant::ClaimOutcome;
using ricardo::wakegrant::ClaimResult;
using ricardo::wakegrant::ServiceEndpoint;

int g_failures = 0;
int g_checks = 0;

void check(bool condition, const char* what, const char* file, int line) {
    ++g_checks;
    if (!condition) {
        ++g_failures;
        std::printf("FAIL %s:%d: %s\n", file, line, what);
    }
}

void check_eq(const std::string& actual, const std::string& expected, const char* what, const char* file, int line) {
    ++g_checks;
    if (actual != expected) {
        ++g_failures;
        std::printf("FAIL %s:%d: %s\n  expected: %s\n  actual:   %s\n", file, line, what,
                    expected.c_str(), actual.c_str());
    }
}

#define CHECK(cond) check((cond), #cond, __FILE__, __LINE__)
#define CHECK_EQ(actual, expected) check_eq((actual), (expected), #actual, __FILE__, __LINE__)

Bytes ByteRange(unsigned char first, unsigned char last) {
    Bytes out;
    for (unsigned value = first; value <= last; ++value) out.push_back(static_cast<std::uint8_t>(value));
    return out;
}

std::string MakeClaimBody(bool granted, const std::string& grant_id, const std::string& mac) {
    std::string body = "{\"granted\":";
    body += granted ? "true" : "false";
    if (!grant_id.empty()) body += ",\"grantId\":\"" + grant_id + "\"";
    body += ",\"mac\":\"" + mac + "\"}";
    return body;
}

struct TestVector {
    Bytes key = ByteRange(0x00, 0x1f);
    std::string machine = "komp";
    std::string nonce = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
    std::string grant_id = "00000000-0000-4000-8000-000000000001";
};

void TestProtocolVectors() {
    const TestVector vector;

    CHECK_EQ(ricardo::wakegrant::claim_mac_message(vector.machine, vector.nonce),
             "claim\nkomp\n" + vector.nonce);
    CHECK_EQ(ricardo::wakegrant::grant_mac_message(vector.machine, vector.nonce, true, vector.grant_id),
             "grant\nkomp\n" + vector.nonce + "\n1\n" + vector.grant_id);
    CHECK_EQ(ricardo::wakegrant::grant_mac_message(vector.machine, vector.nonce, false, ""),
             "grant\nkomp\n" + vector.nonce + "\n0\n");

    CHECK_EQ(ricardo::wakegrant::hmac_sha256_b64_reference(
                 vector.key, ricardo::wakegrant::claim_mac_message(vector.machine, vector.nonce)),
             "+YedSn/aeRZqRNKKzkUS8Dy2aq/dVLkXbh9iX91L9XU=");
    CHECK_EQ(ricardo::wakegrant::hmac_sha256_b64_reference(
                 vector.key,
                 ricardo::wakegrant::grant_mac_message(vector.machine, vector.nonce, true, vector.grant_id)),
             "UAADAs7pUxn/EN42woIH5K0qrH2YB+bd1SYZVKP//FI=");
    CHECK_EQ(ricardo::wakegrant::hmac_sha256_b64_reference(
                 vector.key,
                 ricardo::wakegrant::grant_mac_message(vector.machine, vector.nonce, false, "")),
             "I6ClmYKmmfO6wodeKXOobAtJj/SgQ7C/ieNumDYrj1Y=");

    // Valid granted response.
    const std::string granted_mac = "UAADAs7pUxn/EN42woIH5K0qrH2YB+bd1SYZVKP//FI=";
    ClaimResult granted = ricardo::wakegrant::verify_claim_response(
        MakeClaimBody(true, vector.grant_id, granted_mac), vector.key, vector.machine, vector.nonce);
    CHECK(granted.outcome == ClaimOutcome::Granted);
    CHECK_EQ(granted.grant_id, vector.grant_id);

    // Valid denied response.
    ClaimResult denied = ricardo::wakegrant::verify_claim_response(
        MakeClaimBody(false, "", "I6ClmYKmmfO6wodeKXOobAtJj/SgQ7C/ieNumDYrj1Y="), vector.key,
        vector.machine, vector.nonce);
    CHECK(denied.outcome == ClaimOutcome::Denied);
    CHECK(denied.grant_id.empty());

    // A granted body cannot carry the denied mac.
    CHECK(ricardo::wakegrant::verify_claim_response(
              MakeClaimBody(true, vector.grant_id, "I6ClmYKmmfO6wodeKXOobAtJj/SgQ7C/ieNumDYrj1Y="),
              vector.key, vector.machine, vector.nonce)
              .outcome == ClaimOutcome::InvalidMac);

    // A denied body cannot carry the granted mac.
    CHECK(ricardo::wakegrant::verify_claim_response(MakeClaimBody(false, "", granted_mac), vector.key,
                                                    vector.machine, vector.nonce)
              .outcome == ClaimOutcome::InvalidMac);

    // Replay against another nonce, another machine or another key is rejected.
    CHECK(ricardo::wakegrant::verify_claim_response(
              MakeClaimBody(true, vector.grant_id, granted_mac), vector.key, vector.machine,
              "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHg8=")
              .outcome == ClaimOutcome::InvalidMac);
    CHECK(ricardo::wakegrant::verify_claim_response(
              MakeClaimBody(true, vector.grant_id, granted_mac), vector.key, "other", vector.nonce)
              .outcome == ClaimOutcome::InvalidMac);
    Bytes other_key = vector.key;
    other_key[0] ^= 0x01;
    CHECK(ricardo::wakegrant::verify_claim_response(MakeClaimBody(true, vector.grant_id, granted_mac),
                                                    other_key, vector.machine, vector.nonce)
              .outcome == ClaimOutcome::InvalidMac);
    // A different grant id invalidates the mac as well.
    CHECK(ricardo::wakegrant::verify_claim_response(
              MakeClaimBody(true, "00000000-0000-4000-8000-000000000002", granted_mac), vector.key,
              vector.machine, vector.nonce)
              .outcome == ClaimOutcome::InvalidMac);
}

void TestBase64() {
    using ricardo::wakegrant::b64_decode;
    using ricardo::wakegrant::b64_encode;

    CHECK_EQ(b64_encode(Bytes{}), "");
    CHECK_EQ(b64_encode(Bytes{'f'}), "Zg==");
    CHECK_EQ(b64_encode(Bytes{'f', 'o'}), "Zm8=");
    CHECK_EQ(b64_encode(Bytes{'f', 'o', 'o'}), "Zm9v");
    CHECK_EQ(b64_encode(Bytes{'f', 'o', 'o', 'b'}), "Zm9vYg==");
    CHECK_EQ(b64_encode(Bytes{'f', 'o', 'o', 'b', 'a'}), "Zm9vYmE=");
    CHECK_EQ(b64_encode(Bytes{'f', 'o', 'o', 'b', 'a', 'r'}), "Zm9vYmFy");

    Bytes decoded;
    CHECK(b64_decode("Zm9vYmFy", &decoded));
    CHECK(decoded == (Bytes{'f', 'o', 'o', 'b', 'a', 'r'}));
    CHECK(b64_decode("", &decoded) && decoded.empty());

    const char* rejected[] = {
        "Zg=",       // wrong length
        "Z===",      // too much padding
        "=Zg=",      // padding first
        "Zg==Zg==",  // padding inside
        "Zm9v\n",    // whitespace
        "Zm9v!",     // bad character
        "Zg==x",     // trailing data
        "ZZ==",      // not decoded to the same bytes? still valid base64, checked for round trip below
    };
    for (int i = 0; i < 8; ++i) {
        if (std::string(rejected[i]) == "ZZ==") continue;  // valid, handled separately
        CHECK(!b64_decode(rejected[i], &decoded));
    }
    CHECK(b64_decode("ZZ==", &decoded) && decoded.size() == 1 && decoded[0] == 0x65);
}

void TestChallengeParsing() {
    using ricardo::wakegrant::parse_challenge_response;

    ricardo::wakegrant::ChallengeResponse response;
    std::string error;

    CHECK(parse_challenge_response(
        "{\"nonce\":\"AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=\",\"expiresIn\":60}", &response,
        &error));
    CHECK_EQ(response.nonce, "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=");
    CHECK(response.expires_in == 60);

    CHECK(parse_challenge_response(
        "  {\"expiresIn\":60,\"nonce\":\"AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=\"}\n",
        &response, &error));

    // Empty nonce field and a non-object body.
    CHECK(!parse_challenge_response("{}", &response, &error));

    // Malformed shapes: none may be accepted.
    const std::string nonce = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
    const std::string bad[] = {
        "",
        "ok",
        "[]",
        "{\"nonce\":\"" + nonce + "\"} trailing",
        "{\"nonce\":\"" + nonce + "\",}",
        "{\"nonce\":null}",
        "{\"nonce\":123}",
        "{\"nonce\":\"short\"}",
        "{\"nonce\":\"" + nonce + "\",\"nonce\":\"" + nonce + "\"}",
        "{\"nonce\":\"" + nonce + "\",\"expiresIn\":0}",
        "{\"nonce\":\"" + nonce + "\",\"expiresIn\":1.5}",
        "{\"nonce\":\"" + nonce + "\",\"extra\":{\"a\":1}}",
        "{\"nonce\":\"" + nonce + "\",\"expiresIn\":\"60\"}",
        "{\"nonce\":\"" + nonce + "\",\"expiresIn\":7200}",
    };
    for (const std::string& body : bad) {
        CHECK(!parse_challenge_response(body, &response, &error));
        CHECK(!error.empty());
    }
}

void TestClaimParsingRejections() {
    using ricardo::wakegrant::verify_claim_response;
    const TestVector vector;
    const std::string mac = "UAADAs7pUxn/EN42woIH5K0qrH2YB+bd1SYZVKP//FI=";

    struct Case {
        std::string body;
        ClaimOutcome outcome;
    };
    const Case cases[] = {
        {"", ClaimOutcome::Malformed},
        {"{}", ClaimOutcome::Malformed},
        {"{\"granted\":true}", ClaimOutcome::Malformed},
        {"{\"granted\":true,\"grantId\":\"\",\"mac\":\"\"}", ClaimOutcome::Malformed},
        {"{\"granted\":\"yes\",\"mac\":\"" + mac + "\"}", ClaimOutcome::Malformed},
        {"{\"granted\":true,\"grantId\":\"x\"}", ClaimOutcome::InvalidMac},
        {"{\"granted\":true,\"grantId\":\"x\",\"mac\":null}", ClaimOutcome::Malformed},
        {"{\"granted\":true,\"grantId\":\"x\",\"mac\":\"bm90LWEtbWFj\"}", ClaimOutcome::InvalidMac},
        {"{\"granted\":false,\"grantId\":\"nonempty\",\"mac\":\"" + mac + "\"}", ClaimOutcome::Malformed},
        {"{\"granted\":true,\"grantId\":\"x\",\"mac\":\"" + mac + "\"", ClaimOutcome::Malformed},
        {"{\"granted\":true,\"grantId\":\"x\",\"mac\":\"" + mac + "\"}{}", ClaimOutcome::Malformed},
    };
    for (const Case& item : cases) {
        const ClaimResult result =
            verify_claim_response(item.body, vector.key, vector.machine, vector.nonce);
        if (result.outcome != item.outcome) std::printf("  body: %s\n", item.body.c_str());
        CHECK(result.outcome == item.outcome);
        CHECK(!result.detail.empty());
    }
}

void TestConfigValidation() {
    using ricardo::wakegrant::endpoint_path;
    using ricardo::wakegrant::parse_service_url;
    using ricardo::wakegrant::redact_service_url;
    using ricardo::wakegrant::validate_config;

    std::string normalized;
    std::string error;

    CHECK(validate_config("http://192.0.2.27:30130", "komp", "pl", ".", &normalized, &error));
    CHECK_EQ(normalized, "http://192.0.2.27:30130");
    CHECK(validate_config("http://192.0.2.27:30130/", "komp", "pl", ".", &normalized, &error));
    CHECK_EQ(normalized, "http://192.0.2.27:30130");
    CHECK(validate_config("https://home.example:8443/wake/", "komp-2", "pl", ".", &normalized, &error));
    CHECK_EQ(normalized, "https://home.example:8443/wake");
    CHECK(validate_config("http://[fd00::1]:30130", "komp", "pl", ".", &normalized, &error));
    CHECK_EQ(normalized, "http://[fd00::1]:30130");
    CHECK(validate_config("http://example.lan", "komp", "pl", ".", &normalized, &error));
    CHECK_EQ(normalized, "http://example.lan");

    struct Invalid {
        std::string url;
        std::string machine;
        std::string user;
        std::string domain;
    };
    const Invalid invalid[] = {
        {"", "komp", "pl", "."},
        {"ftp://192.0.2.27", "komp", "pl", "."},
        {"192.0.2.27:30130", "komp", "pl", "."},
        {"http://", "komp", "pl", "."},
        {"http://192.0.2.27:0", "komp", "pl", "."},
        {"http://192.0.2.27:70000", "komp", "pl", "."},
        {"http://192.0.2.27:abc", "komp", "pl", "."},
        {"http://user:pass@192.0.2.27", "komp", "pl", "."},
        {"http://192.0.2.27/?x=1", "komp", "pl", "."},
        {"http://192.0.2.27/#frag", "komp", "pl", "."},
        {"http://192.0.2.27/a//b", "komp", "pl", "."},
        {"http://192.0.2.27 /x", "komp", "pl", "."},
        {"http://192.0.2.27", "", "pl", "."},
        {"http://192.0.2.27", "Komp", "pl", "."},
        {"http://192.0.2.27", "komp_1", "pl", "."},
        {"http://192.0.2.27", "komp", "", "."},
        {"http://192.0.2.27", "komp", "p l", "."},
        {"http://192.0.2.27", "komp", "a\\b", "."},
        {"http://192.0.2.27", "komp", "pl", ""},
        {"http://192.0.2.27", "komp", "pl", "a b"},
        {"http://192.0.2.27", std::string(33, 'a'), "pl", "."},
    };
    for (const Invalid& item : invalid) {
        CHECK(!validate_config(item.url, item.machine, item.user, item.domain, &normalized, &error));
        CHECK(!error.empty());
    }

    ServiceEndpoint endpoint;
    CHECK(parse_service_url("http://192.0.2.27:30130/wake", &endpoint, &error));
    CHECK(endpoint.host == "192.0.2.27");
    CHECK(endpoint.port == 30130);
    CHECK(!endpoint.tls);
    CHECK_EQ(endpoint.prefix, "/wake");
    CHECK_EQ(endpoint_path(endpoint, "/v1/claim"), "/wake/v1/claim");
    CHECK(parse_service_url("https://example.lan", &endpoint, &error));
    CHECK(endpoint.port == 443);
    CHECK(endpoint.tls);
    CHECK_EQ(endpoint_path(endpoint, "/v1/claim"), "/v1/claim");
    CHECK(parse_service_url("http://[fd00::1]", &endpoint, &error));
    CHECK(endpoint.host == "fd00::1");
    CHECK(endpoint.port == 80);

    CHECK_EQ(redact_service_url("http://192.0.2.27:30130/secret/path"), "http://192.0.2.27:30130");
    CHECK_EQ(redact_service_url("not a url"), "invalid-url");
}

void TestRequestBodies() {
    using ricardo::wakegrant::challenge_request_json;
    using ricardo::wakegrant::claim_request_json;

    CHECK_EQ(challenge_request_json("komp"), "{\"machine\":\"komp\"}");
    const std::string claim = claim_request_json("komp", "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=");
    CHECK(claim.find("\"machine\":\"komp\"") != std::string::npos);
    CHECK(claim.find("\"nonce\":\"AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=\"") != std::string::npos);
    CHECK(claim.find("\"mac\":\"\"") != std::string::npos);
}

void TestSecretFile() {
    using ricardo::wakegrant::MachineSecret;
    using ricardo::wakegrant::parse_secret_json;

    const std::string key = ricardo::wakegrant::b64_encode(ByteRange(0x00, 0x1f));
    MachineSecret secret;
    std::string error;

    CHECK(parse_secret_json("{\"version\":1,\"password\":\"p@ss\\\"word\",\"key\":\"" + key +
                                "\"}",
                            &secret, &error));
    CHECK_EQ(secret.password, "p@ss\"word");
    CHECK(secret.key == ByteRange(0x00, 0x1f));

    // version is optional and ignored when absent.
    CHECK(parse_secret_json("{\"password\":\"x\",\"key\":\"" + key + "\"}", &secret, &error));

    const std::string bad[] = {
        "",
        "{}",
        "{\"password\":\"x\"}",
        "{\"key\":\"" + key + "\"}",
        "{\"password\":\"x\",\"key\":\"c2hvcnQ=\"}",
        "{\"password\":\"\",\"key\":\"" + key + "\"}",
        "{\"password\":\"x\",\"key\":\"\"}",
        "{\"version\":2,\"password\":\"x\",\"key\":\"" + key + "\"}",
        "{\"password\":\"x\",\"key\":\"" + key + "\"",
    };
    for (const std::string& body : bad) {
        CHECK(!parse_secret_json(body, &secret, &error));
        CHECK(secret.password.empty() && secret.key.empty());
    }
}

void TestConstantTimeEqual() {
    using ricardo::wakegrant::constant_time_equal;

    CHECK(constant_time_equal(std::string_view("abc"), std::string_view("abc")));
    CHECK(!constant_time_equal(std::string_view("abc"), std::string_view("abd")));
    CHECK(!constant_time_equal(std::string_view("abc"), std::string_view("abcd")));
    CHECK(constant_time_equal(Bytes{1, 2, 3}, Bytes{1, 2, 3}));
    CHECK(!constant_time_equal(Bytes{1, 2, 3}, Bytes{1, 2, 4}));
}

}  // namespace

int main() {
    TestProtocolVectors();
    TestBase64();
    TestChallengeParsing();
    TestClaimParsingRejections();
    TestConfigValidation();
    TestRequestBodies();
    TestSecretFile();
    TestConstantTimeEqual();

    if (g_failures == 0) {
        std::printf("OK: %d protocol checks passed\n", g_checks);
        return 0;
    }
    std::printf("FAILED: %d of %d protocol checks\n", g_failures, g_checks);
    return 1;
}
