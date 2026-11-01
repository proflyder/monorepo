// Package redact removes secret-like values from text before it is returned to
// a model or written to a report.
package redact

import "regexp"

// Placeholder replaces every detected secret value.
const Placeholder = "[REDACTED]"

// patterns is ordered: later rules catch secrets the labeled rules did not
// recognize.
//
// Invariant: a pattern may declare at most ONE capture group, and that group
// must contain only the non-secret prefix (a header name, `token=`, the bot id
// before `:`, …). The secret itself is always outside the group, so a
// replacement can never reintroduce it.
var patterns = []*regexp.Regexp{
	// Authorization headers, any scheme. Group 1: the header prefix.
	regexp.MustCompile(`(?i)(authorization\s*[:=]\s*(?:bearer|basic|token)?\s*)[^\s"',;]+`),
	// Bare bearer tokens. Group 1: the word "bearer".
	regexp.MustCompile(`(?i)\b(bearer\s+)[A-Za-z0-9._~+/=-]{8,}`),
	// key=value / key: value pairs for common secret names. Group 1: the key.
	regexp.MustCompile(`(?i)\b((?:token|password|passwd|secret|api[_-]?key|access[_-]?key|private[_-]?key)\s*[:=]\s*["']?)[^\s"',;&]+`),
	// JSON Web Tokens. No group: the whole match is the secret.
	regexp.MustCompile(`\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b`),
	// Telegram bot tokens and similar "<digits>:<secret>" forms. Group 1 is
	// only the numeric bot id and the colon; the token stays outside the group.
	regexp.MustCompile(`\b([0-9]{6,}:)[A-Za-z0-9_-]{30,}\b`),
	// Long hexadecimal blobs (keys, digests, checksums printed by tools).
	regexp.MustCompile(`\b[0-9a-fA-F]{32,}\b`),
	// Long base64-ish blobs. This over-approximates on purpose: a 33-character
	// unbroken token is almost never ordinary log text.
	regexp.MustCompile(`\b[A-Za-z0-9+/_-]{33,}={0,2}\b`),
}

// Text redacts every secret-like value in s.
func Text(s string) string {
	for _, re := range patterns {
		s = redactWith(re, s)
	}
	return s
}

// redactWith applies one pattern. It keeps group 1 (the non-secret prefix) when
// present and replaces everything else, so the secret can never leak through a
// capture group.
func redactWith(re *regexp.Regexp, s string) string {
	return re.ReplaceAllStringFunc(s, func(match string) string {
		groups := re.FindStringSubmatch(match)
		if len(groups) > 1 {
			return groups[1] + Placeholder
		}
		return Placeholder
	})
}
