package bb

import (
	"crypto/hmac"
	"crypto/sha1"
	"encoding/base64"
	"fmt"
	"net"
	"os"
	"strings"

	"golang.org/x/crypto/ssh"
	"golang.org/x/crypto/ssh/knownhosts"
)

// hostKeyAlgorithms lists the host key algorithms the client may negotiate,
// derived from the keys pinned in knownHostsFile for address.
//
// Without this restriction the Go SSH client offers every compiled-in host key
// algorithm in its own preference order, so a server may select a type that is
// not pinned (for example RSA while known_hosts only pins ed25519). Such a
// handshake can never pass host key verification. Returning only the pinned
// types makes the client ask the server for keys it can actually check.
//
// An empty result means known_hosts has no entry for the address; the caller
// then keeps the library defaults and the host key callback still refuses the
// unknown host.
func hostKeyAlgorithms(knownHostsFile, address string) ([]string, error) {
	data, err := os.ReadFile(knownHostsFile)
	if err != nil {
		return nil, fmt.Errorf("bb: read known_hosts: %w", err)
	}
	return hostKeyAlgorithmsFor(string(data), address), nil
}

// hostKeyAlgorithmsFor is hostKeyAlgorithms over already-read file contents.
func hostKeyAlgorithmsFor(content, address string) []string {
	host, port := splitKnownHostsAddress(address)
	seen := make(map[string]bool)
	var algos []string
	for _, line := range strings.Split(content, "\n") {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		fields := strings.Fields(line)
		if len(fields) == 0 {
			continue
		}
		// A marker is a per-line attribute, not part of the host pattern.
		if strings.HasPrefix(fields[0], "@") {
			// @revoked only blacklists one key and @cert-authority pins a CA
			// whose own type says nothing about the host certificate type, so
			// neither can tell which host key algorithms to offer.
			continue
		}
		if len(fields) < 3 {
			continue
		}
		if !knownHostsPatternMatches(fields[0], host, port) {
			continue
		}
		for _, algo := range algorithmsForKeyType(fields[1]) {
			if !seen[algo] {
				seen[algo] = true
				algos = append(algos, algo)
			}
		}
	}
	return algos
}

// splitKnownHostsAddress normalizes host:port into the parts known_hosts
// matches against: the host and the port, with "22" implicit.
func splitKnownHostsAddress(address string) (host, port string) {
	normalized := knownhosts.Normalize(address)
	if h, p, err := net.SplitHostPort(normalized); err == nil {
		return h, p
	}
	return normalized, "22"
}

// normalizedHost renders host and port the way known_hosts stores them.
func normalizedHost(host, port string) string {
	if port == "" || port == "22" {
		return host
	}
	return "[" + host + "]:" + port
}

// knownHostsPatternMatches applies OpenSSH host pattern semantics: a
// comma-separated list, where "!" negates and "*" / "?" are wildcards. Hashed
// entries (|1|salt|hash) are compared through the same HMAC-SHA1 scheme.
func knownHostsPatternMatches(pattern, host, port string) bool {
	matched := false
	for _, p := range strings.Split(pattern, ",") {
		if p == "" {
			continue
		}
		negate := false
		if p[0] == '!' {
			negate, p = true, p[1:]
			if p == "" {
				continue
			}
		}
		var ok bool
		if strings.HasPrefix(p, "|1|") {
			ok = hashedHostMatches(p, host, port)
		} else {
			ok = plainHostPatternMatches(p, host, port)
		}
		if !ok {
			continue
		}
		if negate {
			return false
		}
		matched = true
	}
	return matched
}

func plainHostPatternMatches(pattern, host, port string) bool {
	patternHost, patternPort := pattern, "22"
	if h, p, err := net.SplitHostPort(pattern); err == nil {
		patternHost, patternPort = h, p
	}
	return patternPort == port && wildcardMatch(patternHost, host)
}

func hashedHostMatches(pattern, host, port string) bool {
	parts := strings.Split(pattern, "|")
	if len(parts) != 4 || parts[1] != "1" {
		return false
	}
	salt, err := base64.StdEncoding.DecodeString(parts[2])
	if err != nil {
		return false
	}
	want, err := base64.StdEncoding.DecodeString(parts[3])
	if err != nil {
		return false
	}
	mac := hmac.New(sha1.New, salt)
	mac.Write([]byte(normalizedHost(host, port)))
	return hmac.Equal(mac.Sum(nil), want)
}

// wildcardMatch follows OpenSSH's matching: "*" spans any characters
// (including separators) and "?" matches exactly one.
func wildcardMatch(pattern, str string) bool {
	for {
		if len(pattern) == 0 {
			return len(str) == 0
		}
		if len(str) == 0 {
			return false
		}
		switch pattern[0] {
		case '*':
			if len(pattern) == 1 {
				return true
			}
			for i := range str {
				if wildcardMatch(pattern[1:], str[i:]) {
					return true
				}
			}
			return false
		case '?':
			pattern, str = pattern[1:], str[1:]
		default:
			if pattern[0] != str[0] {
				return false
			}
			pattern, str = pattern[1:], str[1:]
		}
	}
}

// algorithmsForKeyType expands one known_hosts key type into the negotiation
// algorithm names that can verify it. RSA keys sign with SHA-2 first and fall
// back to ssh-rsa; certificates follow the same split.
func algorithmsForKeyType(keyType string) []string {
	switch keyType {
	case ssh.KeyAlgoRSA:
		return []string{ssh.KeyAlgoRSASHA256, ssh.KeyAlgoRSASHA512, ssh.KeyAlgoRSA}
	case ssh.CertAlgoRSAv01:
		return []string{ssh.CertAlgoRSASHA256v01, ssh.CertAlgoRSASHA512v01, ssh.CertAlgoRSAv01}
	default:
		return []string{keyType}
	}
}
