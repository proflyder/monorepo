// hmac-selftest.exe: checks the Windows CNG HMAC layer (hmac_bcrypt.cpp)
// against the pinned PROTOCOL.md test vector. The portable unit tests use the
// reference HMAC, so this is the only check of the code LogonUI actually runs.

#include <cstdio>
#include <string>

#include "protocol/protocol.h"

using namespace ricardo::wakegrant;

int main() {
    Bytes key;
    for (int i = 0; i < 32; ++i) key.push_back(static_cast<unsigned char>(i));
    const std::string nonce = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
    const std::string grant = "00000000-0000-4000-8000-000000000001";
    struct Case { const char* name; std::string message; const char* expected; } cases[] = {
        {"claim", claim_mac_message("komp", nonce), "+YedSn/aeRZqRNKKzkUS8Dy2aq/dVLkXbh9iX91L9XU="},
        {"granted", grant_mac_message("komp", nonce, true, grant), "UAADAs7pUxn/EN42woIH5K0qrH2YB+bd1SYZVKP//FI="},
        {"denied", grant_mac_message("komp", nonce, false, ""), "I6ClmYKmmfO6wodeKXOobAtJj/SgQ7C/ieNumDYrj1Y="},
    };
    int failed = 0;
    for (const Case& c : cases) {
        const std::string got = hmac_sha256_b64(key, c.message);
        const bool ok = got == c.expected;
        std::printf("  %-8s %s\n", c.name, ok ? "ok" : "FAIL");
        if (!ok) ++failed;
    }
    std::puts(failed == 0 ? "PASS" : "FAIL");
    return failed == 0 ? 0 : 1;
}
