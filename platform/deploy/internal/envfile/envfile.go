// Package envfile parses the private KEY=VALUE files the deploy scripts use.
//
// The format is deliberately tiny and matches the parsers this replaces:
//
//	# comment
//	KEY=value
//	QUOTED="value with spaces"
//	export EXPORTED=value
//
// Every line is trimmed; blank lines and lines starting with `#` are skipped.
// Values may be wrapped in one pair of single or double quotes; the quotes are
// stripped exactly like Python's `str.strip('"').strip("'")` (repeated leading
// and trailing quote characters of the same pair are all removed). No escape
// sequences, interpolation or multi-line values exist, so a value is always a
// literal.
//
// Two readers are provided because the scripts had two:
//
//   - Parse is the strict reader used by the renderer. A malformed line, a
//     missing `=` or a duplicate key is an error, so a half-understood env
//     file can never be rendered.
//   - Lookup is the permissive reader used by `env get`. It returns the first
//     matching key and an empty value when the key is absent, exactly like the
//     shell `env_value` helpers it replaces.
package envfile

import (
	"fmt"
	"os"
	"strings"
)

// Parse reads path and returns every KEY=VALUE pair in file order.
//
// Errors name the file and 1-based line number:
//
//	<path>:<line>: expected KEY=VALUE
//	<path>:<line>: duplicate key <key>
func Parse(path string) (map[string]string, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	values := make(map[string]string)
	for lineno, rawLine := range strings.Split(string(raw), "\n") {
		line := strings.TrimSpace(rawLine)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		eq := strings.Index(line, "=")
		if eq < 0 {
			return nil, fmt.Errorf("%s:%d: expected KEY=VALUE", path, lineno+1)
		}
		key := NormalizeKey(line[:eq])
		if _, duplicate := values[key]; duplicate {
			return nil, fmt.Errorf("%s:%d: duplicate key %s", path, lineno+1, key)
		}
		values[key] = Unquote(line[eq+1:])
	}
	return values, nil
}

// Lookup returns the value of key from path.
//
// It mirrors the shell helper: the first matching line wins, malformed lines
// are ignored, a duplicate key is not an error and a missing key returns
// ("", false, nil). Read errors are returned as-is.
func Lookup(path, key string) (string, bool, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return "", false, err
	}
	for _, rawLine := range strings.Split(string(raw), "\n") {
		line := strings.TrimSpace(rawLine)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		eq := strings.Index(line, "=")
		if eq < 0 {
			continue
		}
		if NormalizeKey(line[:eq]) != key {
			continue
		}
		return Unquote(line[eq+1:]), true, nil
	}
	return "", false, nil
}

// NormalizeKey trims surrounding whitespace and an optional `export ` prefix,
// so `export KEY=value` and `KEY=value` are the same entry.
func NormalizeKey(prefix string) string {
	key := strings.TrimSpace(prefix)
	if strings.HasPrefix(key, "export ") {
		key = strings.TrimSpace(strings.TrimPrefix(key, "export "))
	}
	return key
}

// Unquote trims the value and removes one pair of surrounding quotes.
//
// The order matters: it reproduces the Python helpers byte for byte, including
// their behaviour on nested quotes such as `"\"'x'\""`.
func Unquote(value string) string {
	value = strings.TrimSpace(value)
	value = strings.Trim(value, `"`)
	value = strings.Trim(value, "'")
	return value
}
