#include "http.h"

#include <windows.h>
#include <winhttp.h>

#include <algorithm>
#include <vector>

namespace ricardo {
namespace komp {
namespace {

constexpr int kResolveTimeoutMs = 2000;
constexpr int kConnectTimeoutMs = 2000;
constexpr int kSendTimeoutMs = 4000;
constexpr int kReceiveTimeoutMs = 4000;
constexpr DWORD kMaxResponseBytes = 16 * 1024;

constexpr wchar_t kUserAgent[] = L"RicardoKompLogon/1.0 (Windows LogonUI)";

std::wstring Widen(const std::string& text) {
    if (text.empty()) return std::wstring();
    const int size = MultiByteToWideChar(CP_UTF8, 0, text.data(), static_cast<int>(text.size()),
                                         nullptr, 0);
    if (size <= 0) return std::wstring();
    std::wstring out(static_cast<size_t>(size), L'\0');
    MultiByteToWideChar(CP_UTF8, 0, text.data(), static_cast<int>(text.size()), out.data(), size);
    return out;
}

class WinHttpHandle {
  public:
    WinHttpHandle() = default;
    explicit WinHttpHandle(HINTERNET handle) : handle_(handle) {}
    ~WinHttpHandle() {
        if (handle_ != nullptr) WinHttpCloseHandle(handle_);
    }
    WinHttpHandle(const WinHttpHandle&) = delete;
    WinHttpHandle& operator=(const WinHttpHandle&) = delete;
    HINTERNET get() const { return handle_; }

  private:
    HINTERNET handle_ = nullptr;
};

}  // namespace

HttpResult HttpPostJson(const wakegrant::ServiceEndpoint& endpoint,
                        std::string_view path,
                        std::string_view body) {
    HttpResult result;
    const ULONGLONG started = GetTickCount64();
    auto fail = [&](const char* reason) {
        result.error = reason;
        result.elapsed_ms = static_cast<unsigned>(GetTickCount64() - started);
        return result;
    };

    const std::wstring host = Widen(endpoint.host);
    const std::wstring target = Widen(std::string(path));
    if (host.empty() || target.empty()) {
        return fail("invalid_target");
    }

    WinHttpHandle session(WinHttpOpen(kUserAgent, WINHTTP_ACCESS_TYPE_NO_PROXY,
                                      WINHTTP_NO_PROXY_NAME, WINHTTP_NO_PROXY_BYPASS, 0));
    if (session.get() == nullptr) {
        return fail("session_open_failed");
    }
    WinHttpSetTimeouts(session.get(), kResolveTimeoutMs, kConnectTimeoutMs, kSendTimeoutMs,
                       kReceiveTimeoutMs);

    DWORD disable = WINHTTP_DISABLE_REDIRECTS | WINHTTP_DISABLE_COOKIES;
    WinHttpSetOption(session.get(), WINHTTP_OPTION_DISABLE_FEATURE, &disable, sizeof(disable));

    WinHttpHandle connection(WinHttpConnect(session.get(), host.c_str(), endpoint.port, 0));
    if (connection.get() == nullptr) {
        return fail("connect_failed");
    }

    DWORD flags = endpoint.tls ? WINHTTP_FLAG_SECURE : 0;
    WinHttpHandle request(WinHttpOpenRequest(connection.get(), L"POST", target.c_str(), nullptr,
                                            WINHTTP_NO_REFERER, WINHTTP_DEFAULT_ACCEPT_TYPES,
                                            flags));
    if (request.get() == nullptr) {
        return fail("request_open_failed");
    }
    // Belt and braces: a redirect must never carry the mac to another host.
    DWORD redirect_policy = WINHTTP_OPTION_REDIRECT_POLICY_NEVER;
    WinHttpSetOption(request.get(), WINHTTP_OPTION_REDIRECT_POLICY, &redirect_policy,
                     sizeof(redirect_policy));
    WinHttpSetOption(request.get(), WINHTTP_OPTION_DISABLE_FEATURE, &disable, sizeof(disable));

    const wchar_t headers[] = L"Content-Type: application/json\r\nAccept: application/json\r\n";
    const BOOL sent = WinHttpSendRequest(
        request.get(), headers, static_cast<DWORD>(-1), const_cast<char*>(body.data()),
        static_cast<DWORD>(body.size()), static_cast<DWORD>(body.size()), 0);
    if (!sent) {
        return fail("send_failed");
    }
    if (!WinHttpReceiveResponse(request.get(), nullptr)) {
        return fail("receive_failed");
    }

    DWORD status = 0;
    DWORD status_size = sizeof(status);
    if (!WinHttpQueryHeaders(request.get(),
                             WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                             WINHTTP_HEADER_NAME_BY_INDEX, &status, &status_size,
                             WINHTTP_NO_HEADER_INDEX)) {
        return fail("status_query_failed");
    }

    std::vector<char> buffer(1024);
    for (;;) {
        DWORD available = 0;
        if (!WinHttpQueryDataAvailable(request.get(), &available)) {
            return fail("read_failed");
        }
        if (available == 0) break;
        if (result.body.size() >= kMaxResponseBytes) break;
        if (available > kMaxResponseBytes) available = kMaxResponseBytes;
        if (available > buffer.size()) buffer.resize(available);
        DWORD read = 0;
        if (!WinHttpReadData(request.get(), buffer.data(), available, &read)) {
            return fail("read_failed");
        }
        if (read == 0) break;
        const std::size_t room =
            kMaxResponseBytes - std::min<std::size_t>(result.body.size(), kMaxResponseBytes);
        result.body.append(buffer.data(), std::min<std::size_t>(read, room));
        if (result.body.size() >= kMaxResponseBytes) break;
    }

    result.ok = true;
    result.status = static_cast<int>(status);
    result.elapsed_ms = static_cast<unsigned>(GetTickCount64() - started);
    return result;
}

}  // namespace komp
}  // namespace ricardo
