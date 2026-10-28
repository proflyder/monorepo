package httpapi

import (
	"errors"
	"fmt"
	"net/http"
	"strings"
)

// Error is an API failure. It deliberately carries no request URL, headers or
// response body: only the operation, HTTP status, Connect error code and a
// sanitized, truncated message.
type Error struct {
	Op         string
	StatusCode int
	Code       string
	Message    string
	Err        error
}

func (e *Error) Unwrap() error { return e.Err }

func (e *Error) Error() string {
	var parts []string
	op := e.Op
	if op == "" {
		op = "request"
	}
	parts = append(parts, op)
	if e.StatusCode != 0 {
		parts = append(parts, fmt.Sprintf("status=%d", e.StatusCode))
	}
	if e.Code != "" {
		parts = append(parts, "code="+e.Code)
	}
	if e.Message != "" {
		parts = append(parts, e.Message)
	}
	if e.Err != nil {
		parts = append(parts, e.Err.Error())
	}
	return strings.Join(parts, ": ")
}

// Transient reports whether retrying after a lease expiry can reasonably
// succeed: network errors, TLS errors, timeouts, throttling and 5xx responses.
func (e *Error) Transient() bool {
	if e == nil {
		return false
	}
	if e.Err != nil {
		return true
	}
	switch e.StatusCode {
	case http.StatusRequestTimeout, http.StatusTooEarly, http.StatusTooManyRequests:
		return true
	}
	return e.StatusCode >= 500
}

// IsTransient is a convenience wrapper for wrapped errors.
func IsTransient(err error) bool {
	var apiErr *Error
	if errors.As(err, &apiErr) {
		return apiErr.Transient()
	}
	return false
}

// IsPermanent reports whether err is a classified non-transient API failure.
func IsPermanent(err error) bool {
	var apiErr *Error
	if errors.As(err, &apiErr) {
		return !apiErr.Transient()
	}
	return false
}

// StatusCode returns the HTTP status carried by err, or 0.
func StatusCode(err error) int {
	var apiErr *Error
	if errors.As(err, &apiErr) {
		return apiErr.StatusCode
	}
	return 0
}

// maxMessage bounds server-provided text kept in logs and ack details.
const maxMessage = 200

// Sanitize strips the bearer token from a server-provided message and bounds
// its length. Tokens never appear in our own messages, but a proxy or the
// server may echo request data.
func Sanitize(message, token string) string {
	message = strings.TrimSpace(message)
	if token != "" {
		message = strings.ReplaceAll(message, token, "***")
	}
	message = strings.Join(strings.Fields(message), " ")
	if len(message) > maxMessage {
		message = message[:maxMessage] + "…"
	}
	return message
}
