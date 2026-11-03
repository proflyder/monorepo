#include "log.h"

#include <windows.h>

#include <cstdio>
#include <cwchar>

namespace ricardo {
namespace komp {
namespace {

constexpr wchar_t kDataDir[] = L"C:\\ProgramData\\Ricardo\\KompLogon";
constexpr wchar_t kLogPath[] = L"C:\\ProgramData\\Ricardo\\KompLogon\\logon.log";
constexpr wchar_t kRotatedPath[] = L"C:\\ProgramData\\Ricardo\\KompLogon\\logon.log.1";
constexpr unsigned long long kMaxLogBytes = 256ull * 1024ull;

SRWLOCK g_log_lock = SRWLOCK_INIT;
INIT_ONCE g_log_init = INIT_ONCE_STATIC_INIT;

BOOL CALLBACK InitLogLock(PINIT_ONCE, PVOID, PVOID*) {
    // CreateDirectoryW is idempotent for ERROR_ALREADY_EXISTS. The installer
    // owns the ACL; if it is missing we simply cannot log.
    CreateDirectoryW(kDataDir, nullptr);
    return TRUE;
}

void RotateIfNeeded() {
    WIN32_FILE_ATTRIBUTE_DATA data{};
    if (!GetFileAttributesExW(kLogPath, GetFileExInfoStandard, &data)) return;
    const unsigned long long size =
        (static_cast<unsigned long long>(data.nFileSizeHigh) << 32) | data.nFileSizeLow;
    if (size < kMaxLogBytes) return;
    // One generation only: the current log always describes the latest run.
    DeleteFileW(kRotatedPath);
    MoveFileExW(kLogPath, kRotatedPath, MOVEFILE_REPLACE_EXISTING);
}

void Timestamp(char* buffer, size_t size) {
    SYSTEMTIME now{};
    GetSystemTime(&now);
    std::snprintf(buffer, size, "%04u-%02u-%02uT%02u:%02u:%02u.%03uZ",
                  static_cast<unsigned>(now.wYear), static_cast<unsigned>(now.wMonth),
                  static_cast<unsigned>(now.wDay), static_cast<unsigned>(now.wHour),
                  static_cast<unsigned>(now.wMinute), static_cast<unsigned>(now.wSecond),
                  static_cast<unsigned>(now.wMilliseconds));
    buffer[size - 1] = '\0';
}

}  // namespace

std::string SanitizeForLog(std::string_view text) {
    std::string out;
    out.reserve(text.size() < 240 ? text.size() : 240);
    for (const char raw : text) {
        const unsigned char c = static_cast<unsigned char>(raw);
        if (c < 0x20 || c == 0x7F) {
            out.push_back(' ');
        } else {
            out.push_back(static_cast<char>(c));
        }
        if (out.size() >= 240) break;
    }
    return out;
}

void LogEvent(std::string_view event, std::string_view fields) {
    InitOnceExecuteOnce(&g_log_init, InitLogLock, nullptr, nullptr);

    char timestamp[40];
    Timestamp(timestamp, sizeof(timestamp));

    std::string line;
    line.reserve(160 + fields.size());
    line.append(timestamp);
    line.append(" pid=");
    line.append(std::to_string(GetCurrentProcessId()));
    line.append(" tid=");
    line.append(std::to_string(GetCurrentThreadId()));
    line.push_back(' ');
    line.append(SanitizeForLog(event));
    if (!fields.empty()) {
        line.push_back(' ');
        line.append(SanitizeForLog(fields));
    }
    line.push_back('\n');

    AcquireSRWLockExclusive(&g_log_lock);
    RotateIfNeeded();
    HANDLE file = CreateFileW(kLogPath, FILE_APPEND_DATA, FILE_SHARE_READ | FILE_SHARE_WRITE,
                              nullptr, OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file != INVALID_HANDLE_VALUE) {
        DWORD written = 0;
        WriteFile(file, line.data(), static_cast<DWORD>(line.size()), &written, nullptr);
        CloseHandle(file);
    }
    ReleaseSRWLockExclusive(&g_log_lock);
}

}  // namespace komp
}  // namespace ricardo
