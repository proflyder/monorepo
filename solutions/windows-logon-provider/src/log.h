// Append-only diagnostic log for the logon provider.
//
// Every line is "<UTC ISO8601> <pid> <tid> <event> key=value...". Secrets are
// never passed here; every field is sanitized (control characters stripped,
// length capped) so remote data cannot forge log lines.

#pragma once

#include <string>
#include <string_view>

namespace ricardo {
namespace komp {

// Returns the sanitized text (no CR/LF/control characters, at most 240 bytes).
std::string SanitizeForLog(std::string_view text);

// Appends one line. Never throws, never blocks the caller for long: a failure
// to write is silently ignored (logging must not break logon).
void LogEvent(std::string_view event, std::string_view fields = std::string_view());

}  // namespace komp
}  // namespace ricardo
