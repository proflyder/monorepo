// Minimal WinHTTP JSON POST client.
//
// No proxy, no redirects, no cookies, no credential attachment: the wakegrant
// mac is the only authenticator and it must not be relayed anywhere.

#pragma once

#include <string>
#include <string_view>

#include "protocol/protocol.h"

namespace ricardo {
namespace komp {

struct HttpResult {
    bool ok = false;      // transport succeeded (any HTTP status)
    int status = 0;       // HTTP status code, 0 when the transport failed
    std::string body;     // response body, capped at 16 KiB
    std::string error;    // log-safe reason when !ok
    unsigned elapsed_ms = 0;
};

HttpResult HttpPostJson(const wakegrant::ServiceEndpoint& endpoint,
                        std::string_view path,
                        std::string_view body);

}  // namespace komp
}  // namespace ricardo
