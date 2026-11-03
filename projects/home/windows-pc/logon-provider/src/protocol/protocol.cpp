// Portable wakegrant client logic: base64, messages, flat JSON, verification.
//
// No platform dependencies, so the same code is unit-tested on the build host
// and shipped inside the Windows credential provider.

#include "protocol.h"

#include <algorithm>
#include <cstdio>
#include <map>
#include <string>
#include <utility>

namespace ricardo {
namespace wakegrant {
namespace {

constexpr std::size_t kMaxJsonBytes = 64 * 1024;
constexpr std::size_t kNonceBytes = 32;
constexpr std::size_t kMacBytes = 32;

bool is_json_space(char c) {
    return c == ' ' || c == '\t' || c == '\r' || c == '\n';
}

int b64_value(char c) {
    if (c >= 'A' && c <= 'Z') return c - 'A';
    if (c >= 'a' && c <= 'z') return c - 'a' + 26;
    if (c >= '0' && c <= '9') return c - '0' + 52;
    if (c == '+') return 62;
    if (c == '/') return 63;
    return -1;
}

void append_utf8(std::string* out, std::uint32_t code_point) {
    if (code_point <= 0x7F) {
        out->push_back(static_cast<char>(code_point));
    } else if (code_point <= 0x7FF) {
        out->push_back(static_cast<char>(0xC0 | (code_point >> 6)));
        out->push_back(static_cast<char>(0x80 | (code_point & 0x3F)));
    } else if (code_point <= 0xFFFF) {
        out->push_back(static_cast<char>(0xE0 | (code_point >> 12)));
        out->push_back(static_cast<char>(0x80 | ((code_point >> 6) & 0x3F)));
        out->push_back(static_cast<char>(0x80 | (code_point & 0x3F)));
    } else {
        out->push_back(static_cast<char>(0xF0 | (code_point >> 18)));
        out->push_back(static_cast<char>(0x80 | ((code_point >> 12) & 0x3F)));
        out->push_back(static_cast<char>(0x80 | ((code_point >> 6) & 0x3F)));
        out->push_back(static_cast<char>(0x80 | (code_point & 0x3F)));
    }
}

struct JsonValue {
    enum class Kind { String, Number, Boolean } kind = Kind::String;
    std::string text;
    long long number = 0;
    bool boolean = false;
};

using JsonObject = std::map<std::string, JsonValue>;

// Minimal flat-object JSON reader. The protocol only ever exchanges flat
// objects with string, integer, boolean values; anything else is a protocol
// violation and is rejected instead of being tolerated.
class FlatJsonReader {
  public:
    explicit FlatJsonReader(std::string_view body) : body_(body) {}

    bool parse(JsonObject* out, std::string* error) {
        if (body_.size() > kMaxJsonBytes) {
            return Fail(error, "body too large");
        }
        skip_space();
        if (!consume('{')) return Fail(error, "expected object");
        skip_space();
        if (!consume('}')) {
            for (;;) {
                std::string key;
                if (!string_literal(&key)) return Fail(error, "bad key");
                skip_space();
                if (!consume(':')) return Fail(error, "expected ':'");
                skip_space();
                JsonValue value;
                if (!value_literal(&value)) return Fail(error, "unsupported value");
                if (!out->emplace(key, std::move(value)).second) {
                    return Fail(error, "duplicate key");
                }
                skip_space();
                if (consume(',')) {
                    skip_space();
                    continue;
                }
                if (consume('}')) break;
                return Fail(error, "expected ',' or '}'");
            }
        }
        skip_space();
        if (pos_ != body_.size()) return Fail(error, "trailing data");
        return true;
    }

    static bool Fail(std::string* error, const char* text) {
        if (error != nullptr) *error = text;
        return false;
    }

  private:
    void skip_space() {
        while (pos_ < body_.size() && is_json_space(body_[pos_])) ++pos_;
    }

    bool consume(char expected) {
        if (pos_ < body_.size() && body_[pos_] == expected) {
            ++pos_;
            return true;
        }
        return false;
    }

    bool hex4(std::uint32_t* out) {
        if (pos_ + 4 > body_.size()) return false;
        std::uint32_t value = 0;
        for (int i = 0; i < 4; ++i) {
            const char c = body_[pos_ + i];
            value <<= 4;
            if (c >= '0' && c <= '9') {
                value |= static_cast<std::uint32_t>(c - '0');
            } else if (c >= 'a' && c <= 'f') {
                value |= static_cast<std::uint32_t>(c - 'a' + 10);
            } else if (c >= 'A' && c <= 'F') {
                value |= static_cast<std::uint32_t>(c - 'A' + 10);
            } else {
                return false;
            }
        }
        pos_ += 4;
        *out = value;
        return true;
    }

    bool string_literal(std::string* out) {
        out->clear();
        if (!consume('"')) return false;
        while (pos_ < body_.size()) {
            const unsigned char c = static_cast<unsigned char>(body_[pos_]);
            if (c == '"') {
                ++pos_;
                return true;
            }
            if (c < 0x20) return false;
            if (c != '\\') {
                out->push_back(static_cast<char>(c));
                ++pos_;
                continue;
            }
            ++pos_;
            if (pos_ >= body_.size()) return false;
            const char escape = body_[pos_++];
            switch (escape) {
                case '"': out->push_back('"'); break;
                case '\\': out->push_back('\\'); break;
                case '/': out->push_back('/'); break;
                case 'b': out->push_back('\b'); break;
                case 'f': out->push_back('\f'); break;
                case 'n': out->push_back('\n'); break;
                case 'r': out->push_back('\r'); break;
                case 't': out->push_back('\t'); break;
                case 'u': {
                    std::uint32_t unit = 0;
                    if (!hex4(&unit)) return false;
                    if (unit >= 0xD800 && unit <= 0xDBFF) {
                        if (pos_ + 2 > body_.size() || body_[pos_] != '\\' || body_[pos_ + 1] != 'u') {
                            return false;
                        }
                        pos_ += 2;
                        std::uint32_t low = 0;
                        if (!hex4(&low)) return false;
                        if (low < 0xDC00 || low > 0xDFFF) return false;
                        append_utf8(out, 0x10000 + ((unit - 0xD800) << 10) + (low - 0xDC00));
                    } else if (unit >= 0xDC00 && unit <= 0xDFFF) {
                        return false;
                    } else {
                        append_utf8(out, unit);
                    }
                    break;
                }
                default:
                    return false;
            }
        }
        return false;
    }

    bool value_literal(JsonValue* out) {
        if (pos_ >= body_.size()) return false;
        if (body_[pos_] == '"') {
            out->kind = JsonValue::Kind::String;
            return string_literal(&out->text);
        }
        if (body_.compare(pos_, 4, "true") == 0) {
            pos_ += 4;
            out->kind = JsonValue::Kind::Boolean;
            out->boolean = true;
            return true;
        }
        if (body_.compare(pos_, 5, "false") == 0) {
            pos_ += 5;
            out->kind = JsonValue::Kind::Boolean;
            out->boolean = false;
            return true;
        }
        const std::size_t start = pos_;
        if (pos_ < body_.size() && body_[pos_] == '-') ++pos_;
        const std::size_t digits = pos_;
        while (pos_ < body_.size() && body_[pos_] >= '0' && body_[pos_] <= '9') ++pos_;
        if (pos_ == digits) {
            pos_ = start;
            return false;
        }
        if (pos_ - digits > 1 && body_[digits] == '0') return false;
        if (pos_ - digits > 18) return false;  // keeps every value well inside int64
        out->kind = JsonValue::Kind::Number;
        out->number = std::stoll(std::string(body_.substr(start, pos_ - start)));
        return true;
    }

    std::string_view body_;
    std::size_t pos_ = 0;
};

const JsonValue* find_value(const JsonObject& object, std::string_view key) {
    const auto it = object.find(std::string(key));
    if (it == object.end()) return nullptr;
    return &it->second;
}

bool get_string(const JsonObject& object, std::string_view key, std::string* out) {
    const JsonValue* value = find_value(object, key);
    if (value == nullptr || value->kind != JsonValue::Kind::String) return false;
    *out = value->text;
    return true;
}

bool get_bool(const JsonObject& object, std::string_view key, bool* out) {
    const JsonValue* value = find_value(object, key);
    if (value == nullptr || value->kind != JsonValue::Kind::Boolean) return false;
    *out = value->boolean;
    return true;
}

bool is_lower_alnum_dash(unsigned char c) {
    return (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9') || c == '-';
}

bool has_control_or_space(std::string_view text) {
    for (const char raw : text) {
        const unsigned char c = static_cast<unsigned char>(raw);
        if (c < 0x20 || c == 0x7F || c == ' ') return true;
    }
    return false;
}

}  // namespace

// --- base64 -------------------------------------------------------------------

std::string b64_encode(const Bytes& data) {
    static const char kAlphabet[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    std::string out;
    out.reserve(((data.size() + 2) / 3) * 4);
    std::size_t i = 0;
    for (; i + 3 <= data.size(); i += 3) {
        const std::uint32_t chunk =
            (static_cast<std::uint32_t>(data[i]) << 16) |
            (static_cast<std::uint32_t>(data[i + 1]) << 8) |
            static_cast<std::uint32_t>(data[i + 2]);
        out.push_back(kAlphabet[(chunk >> 18) & 0x3F]);
        out.push_back(kAlphabet[(chunk >> 12) & 0x3F]);
        out.push_back(kAlphabet[(chunk >> 6) & 0x3F]);
        out.push_back(kAlphabet[chunk & 0x3F]);
    }
    const std::size_t rest = data.size() - i;
    if (rest == 1) {
        const std::uint32_t chunk = static_cast<std::uint32_t>(data[i]) << 16;
        out.push_back(kAlphabet[(chunk >> 18) & 0x3F]);
        out.push_back(kAlphabet[(chunk >> 12) & 0x3F]);
        out.push_back('=');
        out.push_back('=');
    } else if (rest == 2) {
        const std::uint32_t chunk =
            (static_cast<std::uint32_t>(data[i]) << 16) |
            (static_cast<std::uint32_t>(data[i + 1]) << 8);
        out.push_back(kAlphabet[(chunk >> 18) & 0x3F]);
        out.push_back(kAlphabet[(chunk >> 12) & 0x3F]);
        out.push_back(kAlphabet[(chunk >> 6) & 0x3F]);
        out.push_back('=');
    }
    return out;
}

bool b64_decode(std::string_view text, Bytes* out) {
    if (out == nullptr) return false;
    out->clear();
    if (text.empty()) return true;
    if (text.size() % 4 != 0) return false;
    std::size_t padding = 0;
    if (text[text.size() - 1] == '=') ++padding;
    if (text.size() >= 2 && text[text.size() - 2] == '=') ++padding;
    out->reserve((text.size() / 4) * 3);
    for (std::size_t i = 0; i < text.size(); i += 4) {
        int values[4] = {0, 0, 0, 0};
        for (std::size_t j = 0; j < 4; ++j) {
            const char c = text[i + j];
            const bool padding_position = (i + j) >= text.size() - padding && padding > 0;
            if (c == '=') {
                if (!padding_position) return false;
                values[j] = 0;
                continue;
            }
            const int value = b64_value(c);
            if (value < 0) return false;
            values[j] = value;
        }
        out->push_back(static_cast<std::uint8_t>((values[0] << 2) | (values[1] >> 4)));
        if (text[i + 2] != '=') {
            out->push_back(static_cast<std::uint8_t>(((values[1] & 0x0F) << 4) | (values[2] >> 2)));
        }
        if (text[i + 3] != '=') {
            out->push_back(static_cast<std::uint8_t>(((values[2] & 0x03) << 6) | values[3]));
        }
    }
    return true;
}

bool constant_time_equal(const Bytes& a, const Bytes& b) {
    if (a.size() != b.size()) return false;
    unsigned char diff = 0;
    for (std::size_t i = 0; i < a.size(); ++i) {
        diff |= static_cast<unsigned char>(a[i] ^ b[i]);
    }
    return diff == 0;
}

bool constant_time_equal(std::string_view a, std::string_view b) {
    if (a.size() != b.size()) return false;
    unsigned char diff = 0;
    for (std::size_t i = 0; i < a.size(); ++i) {
        diff |= static_cast<unsigned char>(a[i] ^ b[i]);
    }
    return diff == 0;
}

// --- messages -----------------------------------------------------------------

std::string claim_mac_message(std::string_view machine, std::string_view nonce) {
    std::string message;
    message.reserve(6 + machine.size() + nonce.size());
    message.append("claim\n");
    message.append(machine);
    message.push_back('\n');
    message.append(nonce);
    return message;
}

std::string grant_mac_message(std::string_view machine,
                              std::string_view nonce,
                              bool granted,
                              std::string_view grant_id) {
    std::string message;
    message.reserve(8 + machine.size() + nonce.size() + grant_id.size());
    message.append("grant\n");
    message.append(machine);
    message.push_back('\n');
    message.append(nonce);
    message.push_back('\n');
    message.push_back(granted ? '1' : '0');
    message.push_back('\n');
    message.append(grant_id);
    return message;
}

// --- requests -----------------------------------------------------------------

namespace {

std::string json_escape(std::string_view text) {
    std::string out;
    out.reserve(text.size() + 2);
    for (const char raw : text) {
        const unsigned char c = static_cast<unsigned char>(raw);
        switch (c) {
            case '"': out.append("\\\""); break;
            case '\\': out.append("\\\\"); break;
            case '\b': out.append("\\b"); break;
            case '\f': out.append("\\f"); break;
            case '\n': out.append("\\n"); break;
            case '\r': out.append("\\r"); break;
            case '\t': out.append("\\t"); break;
            default:
                if (c < 0x20) {
                    char buffer[8];
                    std::snprintf(buffer, sizeof(buffer), "\\u%04x", c);
                    out.append(buffer);
                } else {
                    out.push_back(static_cast<char>(c));
                }
        }
    }
    return out;
}

}  // namespace

std::string challenge_request_json(std::string_view machine) {
    return "{\"machine\":\"" + json_escape(machine) + "\"}";
}

std::string claim_request_json(std::string_view machine, std::string_view nonce) {
    // `mac` is filled by the caller: this helper is only used for the request
    // body shape test. The Windows client builds the body with its own key.
    return "{\"machine\":\"" + json_escape(machine) + "\",\"nonce\":\"" + json_escape(nonce) +
           "\",\"mac\":\"\"}";
}

// --- responses ----------------------------------------------------------------

bool parse_challenge_response(std::string_view body, ChallengeResponse* out, std::string* error) {
    out->nonce.clear();
    out->expires_in = 0;
    JsonObject object;
    FlatJsonReader reader(body);
    if (!reader.parse(&object, error)) return false;

    std::string nonce;
    if (!get_string(object, "nonce", &nonce)) {
        if (error != nullptr) *error = "challenge without nonce";
        return false;
    }
    Bytes decoded;
    if (!b64_decode(nonce, &decoded) || decoded.size() != kNonceBytes) {
        if (error != nullptr) *error = "challenge nonce is not 32 base64 bytes";
        return false;
    }
    if (const JsonValue* expires = find_value(object, "expiresIn"); expires != nullptr) {
        if (expires->kind != JsonValue::Kind::Number || expires->number <= 0 || expires->number > 3600) {
            if (error != nullptr) *error = "challenge expiresIn out of range";
            return false;
        }
        out->expires_in = expires->number;
    }
    out->nonce = std::move(nonce);
    return true;
}

ClaimResult verify_claim_response(std::string_view body,
                                  const Bytes& key,
                                  std::string_view machine,
                                  std::string_view nonce) {
    ClaimResult result;

    JsonObject object;
    std::string parse_error;
    FlatJsonReader reader(body);
    if (!reader.parse(&object, &parse_error)) {
        result.outcome = ClaimOutcome::Malformed;
        result.detail = parse_error;
        return result;
    }

    bool granted = false;
    if (!get_bool(object, "granted", &granted)) {
        result.outcome = ClaimOutcome::Malformed;
        result.detail = "claim without boolean granted";
        return result;
    }

    std::string grant_id;
    if (granted) {
        if (!get_string(object, "grantId", &grant_id) || grant_id.empty() || grant_id.size() > 64) {
            result.outcome = ClaimOutcome::Malformed;
            result.detail = "granted claim without grantId";
            return result;
        }
    } else if (const JsonValue* id = find_value(object, "grantId"); id != nullptr) {
        if (id->kind != JsonValue::Kind::String || !id->text.empty()) {
            result.outcome = ClaimOutcome::Malformed;
            result.detail = "denied claim carries a grantId";
            return result;
        }
    }

    std::string mac;
    if (!get_string(object, "mac", &mac)) {
        result.outcome = ClaimOutcome::InvalidMac;
        result.detail = "missing mac";
        return result;
    }
    Bytes received;
    if (!b64_decode(mac, &received) || received.size() != kMacBytes) {
        result.outcome = ClaimOutcome::InvalidMac;
        result.detail = "mac is not 32 base64 bytes";
        return result;
    }

    const std::string expected_b64 =
        hmac_sha256_b64(key, grant_mac_message(machine, nonce, granted, grant_id));
    Bytes expected;
    if (!b64_decode(expected_b64, &expected)) {
        result.outcome = ClaimOutcome::InvalidMac;
        result.detail = "local mac encoding failed";
        return result;
    }
    if (!constant_time_equal(received, expected)) {
        result.outcome = ClaimOutcome::InvalidMac;
        result.detail = "mac mismatch";
        return result;
    }

    if (granted) {
        result.outcome = ClaimOutcome::Granted;
        result.grant_id = std::move(grant_id);
    } else {
        result.outcome = ClaimOutcome::Denied;
    }
    return result;
}

// --- configuration ------------------------------------------------------------

bool parse_service_url(std::string_view url, ServiceEndpoint* out, std::string* error) {
    *out = ServiceEndpoint();
    auto fail = [error](const char* text) {
        if (error != nullptr) *error = text;
        return false;
    };
    if (url.size() > 512) return fail("service url too long");
    if (has_control_or_space(url)) return fail("service url contains whitespace");

    std::string_view rest = url;
    if (rest.substr(0, 7) == "http://") {
        out->tls = false;
        out->port = 80;
        rest.remove_prefix(7);
    } else if (rest.substr(0, 8) == "https://") {
        out->tls = true;
        out->port = 443;
        rest.remove_prefix(8);
    } else {
        return fail("service url must start with http:// or https://");
    }
    if (rest.empty()) return fail("service url without host");
    if (rest.find_first_of("?#@") != std::string_view::npos) {
        return fail("service url must not carry credentials, query or fragment");
    }

    const std::size_t slash = rest.find('/');
    std::string_view authority = rest.substr(0, slash);
    if (slash != std::string_view::npos) {
        out->prefix = std::string(rest.substr(slash));
        while (!out->prefix.empty() && out->prefix.back() == '/') out->prefix.pop_back();
        if (out->prefix.find("//") != std::string::npos) return fail("service url path has an empty segment");
    }
    if (authority.empty()) return fail("service url without host");

    std::string_view host = authority;
    std::string_view port_text;
    if (authority.front() == '[') {
        const std::size_t close = authority.find(']');
        if (close == std::string_view::npos) return fail("service url has an unterminated IPv6 literal");
        host = authority.substr(1, close - 1);
        if (close + 1 < authority.size()) {
            if (authority[close + 1] != ':') return fail("service url has trailing data after IPv6 literal");
            port_text = authority.substr(close + 2);
        }
        if (host.empty()) return fail("service url without host");
        for (const char raw : host) {
            const unsigned char c = static_cast<unsigned char>(raw);
            const bool ok = (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F') ||
                            c == ':' || c == '.';
            if (!ok) return fail("service url has an invalid IPv6 literal");
        }
    } else {
        const std::size_t colon = authority.rfind(':');
        if (colon != std::string_view::npos) {
            host = authority.substr(0, colon);
            port_text = authority.substr(colon + 1);
        }
        if (host.empty()) return fail("service url without host");
        if (host.size() > 253) return fail("service url host too long");
        for (const char raw : host) {
            const unsigned char c = static_cast<unsigned char>(raw);
            const bool ok = (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
                            (c >= '0' && c <= '9') || c == '.' || c == '-';
            if (!ok) return fail("service url has an invalid host character");
        }
        if (host.front() == '.' || host.back() == '.' || host.find("..") != std::string_view::npos) {
            return fail("service url host has an empty label");
        }
    }

    if (!port_text.empty()) {
        if (port_text.size() > 5) return fail("service url port is too long");
        unsigned value = 0;
        for (const char c : port_text) {
            if (c < '0' || c > '9') return fail("service url port is not numeric");
            value = value * 10 + static_cast<unsigned>(c - '0');
        }
        if (value == 0 || value > 65535) return fail("service url port out of range");
        out->port = static_cast<std::uint16_t>(value);
    }

    out->host = std::string(host);
    return true;
}

bool validate_config(std::string_view service_url,
                     std::string_view machine,
                     std::string_view user,
                     std::string_view domain,
                     std::string* normalized_url,
                     std::string* error) {
    normalized_url->clear();
    auto fail = [error](const char* text) {
        if (error != nullptr) *error = text;
        return false;
    };

    if (machine.empty() || machine.size() > 32) return fail("machine must be 1..32 characters");
    for (const char c : machine) {
        if (!is_lower_alnum_dash(static_cast<unsigned char>(c))) {
            return fail("machine must match ^[a-z0-9-]{1,32}$");
        }
    }

    if (user.empty() || user.size() > 256) return fail("UserName must be 1..256 characters");
    if (user.find_first_of("\\\"@") != std::string_view::npos) {
        return fail("UserName must not contain '\\', '\"' or '@'");
    }
    if (has_control_or_space(user)) return fail("UserName contains whitespace or control characters");

    if (domain.empty() || domain.size() > 256) return fail("Domain must be 1..256 characters");
    if (domain.find_first_of("\\\"") != std::string_view::npos) {
        return fail("Domain must not contain '\\' or '\"'");
    }
    if (has_control_or_space(domain)) return fail("Domain contains whitespace or control characters");

    ServiceEndpoint endpoint;
    std::string url_error;
    if (!parse_service_url(service_url, &endpoint, &url_error)) {
        if (error != nullptr) *error = url_error;
        return false;
    }

    std::string result = endpoint.tls ? "https://" : "http://";
    if (endpoint.host.find(':') != std::string::npos) {
        result += "[" + endpoint.host + "]";
    } else {
        result += endpoint.host;
    }
    const std::uint16_t default_port = endpoint.tls ? 443 : 80;
    if (endpoint.port != default_port) {
        result += ":" + std::to_string(endpoint.port);
    }
    result += endpoint.prefix;
    *normalized_url = std::move(result);
    return true;
}

std::string redact_service_url(std::string_view url) {
    ServiceEndpoint endpoint;
    std::string error;
    if (!parse_service_url(url, &endpoint, &error)) return "invalid-url";
    std::string result = endpoint.tls ? "https://" : "http://";
    if (endpoint.host.find(':') != std::string::npos) {
        result += "[" + endpoint.host + "]";
    } else {
        result += endpoint.host;
    }
    result += ":" + std::to_string(endpoint.port);
    return result;
}

std::string endpoint_path(const ServiceEndpoint& endpoint, std::string_view path) {
    std::string result = endpoint.prefix;
    if (path.empty() || path.front() != '/') result.push_back('/');
    result.append(path);
    return result;
}

// --- secret file --------------------------------------------------------------

bool parse_secret_json(std::string_view body, MachineSecret* out, std::string* error) {
    out->password.clear();
    out->key.clear();

    JsonObject object;
    FlatJsonReader reader(body);
    if (!reader.parse(&object, error)) return false;

    if (const JsonValue* version = find_value(object, "version"); version != nullptr) {
        if (version->kind != JsonValue::Kind::Number || version->number != 1) {
            if (error != nullptr) *error = "secret version must be 1";
            return false;
        }
    }

    std::string password;
    if (!get_string(object, "password", &password)) {
        if (error != nullptr) *error = "secret without password";
        return false;
    }
    if (password.empty() || password.size() > 512) {
        if (error != nullptr) *error = "secret password must be 1..512 bytes";
        return false;
    }
    for (const char raw : password) {
        // A password may contain any byte except NUL; JSON escapes keep line
        // breaks representable.
        if (raw == '\0') {
            if (error != nullptr) *error = "secret password contains a NUL";
            return false;
        }
    }

    std::string encoded_key;
    if (!get_string(object, "key", &encoded_key)) {
        if (error != nullptr) *error = "secret without key";
        return false;
    }
    Bytes key;
    if (!b64_decode(encoded_key, &key) || key.size() != 32) {
        if (error != nullptr) *error = "secret key is not 32 base64 bytes";
        return false;
    }

    out->password = std::move(password);
    out->key = std::move(key);
    return true;
}

}  // namespace wakegrant
}  // namespace ricardo
