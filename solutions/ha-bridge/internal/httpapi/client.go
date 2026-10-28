// Package httpapi holds the shared HTTP plumbing for the ha-bridge API clients:
// a transport that keeps TLS verification enabled, an API error type that
// classifies transient failures and never renders credentials, and a small
// sanitizer for server-provided messages.
package httpapi

import (
	"crypto/tls"
	"crypto/x509"
	"fmt"
	"net"
	"net/http"
	"os"
	"time"
)

// WithoutRedirects returns a client that surfaces 3xx responses instead of
// following them. Authenticated clients must never forward a bearer token to a
// redirect target, and a redirect is not a successful call. The provided client
// is cloned, never mutated.
func WithoutRedirects(hc *http.Client) *http.Client {
	if hc == nil {
		hc = &http.Client{}
	}
	if hc.CheckRedirect != nil {
		return hc
	}
	clone := *hc
	clone.CheckRedirect = func(*http.Request, []*http.Request) error {
		return http.ErrUseLastResponse
	}
	return &clone
}

// NewClient builds an HTTP client with TLS verification enabled and redirects
// disabled. An optional CA file adds a private root (for a LAN Home Assistant
// or scheduler with a self-signed certificate); it never disables verification
// and it extends, rather than replaces, the system trust store.
func NewClient(caFile string, timeout time.Duration) (*http.Client, error) {
	tlsConfig := &tls.Config{MinVersion: tls.VersionTLS12}

	if caFile != "" {
		pem, err := os.ReadFile(caFile)
		if err != nil {
			return nil, fmt.Errorf("read CA file: %w", err)
		}
		pool, err := x509.SystemCertPool()
		if err != nil || pool == nil {
			pool = x509.NewCertPool()
		}
		if !pool.AppendCertsFromPEM(pem) {
			return nil, fmt.Errorf("CA file %q contains no usable certificates", caFile)
		}
		tlsConfig.RootCAs = pool
	}

	transport := &http.Transport{
		Proxy: http.ProxyFromEnvironment,
		DialContext: (&net.Dialer{
			Timeout:   5 * time.Second,
			KeepAlive: 30 * time.Second,
		}).DialContext,
		TLSClientConfig:     tlsConfig,
		TLSHandshakeTimeout: 10 * time.Second,
		MaxIdleConns:        8,
		IdleConnTimeout:     60 * time.Second,
	}

	return WithoutRedirects(&http.Client{
		Transport: transport,
		Timeout:   timeout,
	}), nil
}
