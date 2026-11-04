// Command health-exporter is the runtime chart's TCP health exporter.
//
// It replaces `projects/home-assistant/deploy/chart/files/health_exporter.py`
// and keeps its contract: operational TCP checks only, no commands, no
// credentials, no conversation content. Metric names, labels, ports and the
// ENDPOINT_HOST_* environment variables are unchanged, so the existing
// dashboards and alerts keep working.
//
// The binary also serves as its own in-pod client (`-query URL`), because the
// distroless image has no shell, curl or python for the rollout's exec check.
package main

import (
	"flag"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"os"
	"strings"
	"time"
)

// Endpoint is one TCP check. The order is the Python exporter's dict order, so
// the metrics text is stable for scraping and diffing.
type Endpoint struct {
	Name string
	Port int
}

// Endpoints are the services the home runtime exposes on the host network.
var Endpoints = []Endpoint{
	{"ha", 8123},
	{"music_assistant", 8095},
	{"piper", 10200},
	{"whisper", 10300},
	{"gigaam", 10305},
	{"hermes", 8642},
	{"voice_proxy", 8643},
}

// Exporter renders the metrics document.
type Exporter struct {
	// Dial connects to host:port. It is injectable for tests.
	Dial func(host string, port int) error
	// Host resolves the address for an endpoint name (ENDPOINT_HOST_<NAME>).
	Host func(name string) string
	// Now is the clock used for the connect duration. It is injectable so a
	// test can assert formatting without depending on wall time.
	Now func() time.Time
}

// New returns an Exporter that reads ENDPOINT_HOST_* and dials with a 1s
// timeout, matching the Python implementation.
func New() *Exporter {
	return &Exporter{
		Dial: func(host string, port int) error {
			connection, err := net.DialTimeout("tcp", net.JoinHostPort(host, fmt.Sprint(port)), time.Second)
			if err != nil {
				return err
			}
			return connection.Close()
		},
		Host: func(name string) string {
			if host := os.Getenv("ENDPOINT_HOST_" + strings.ToUpper(name)); host != "" {
				return host
			}
			return "127.0.0.1"
		},
		Now: time.Now,
	}
}

// Metrics renders the Prometheus text exposition.
func (e *Exporter) Metrics() []byte {
	lines := []string{
		"# HELP ricardo_endpoint_up Whether a configured TCP endpoint accepts connections.",
		"# TYPE ricardo_endpoint_up gauge",
		"# HELP ricardo_endpoint_connect_seconds TCP connection time, not voice latency.",
		"# TYPE ricardo_endpoint_connect_seconds gauge",
	}
	for _, endpoint := range Endpoints {
		start := e.Now()
		up := 0
		if err := e.Dial(e.Host(endpoint.Name), endpoint.Port); err == nil {
			up = 1
		}
		elapsed := e.Now().Sub(start).Seconds()
		lines = append(lines,
			fmt.Sprintf(`ricardo_endpoint_up{endpoint=%q} %d`, endpoint.Name, up),
			fmt.Sprintf(`ricardo_endpoint_connect_seconds{endpoint=%q} %.6f`, endpoint.Name, elapsed),
		)
	}
	return []byte(strings.Join(lines, "\n") + "\n")
}

// Handler serves /metrics and nothing else.
func (e *Exporter) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("/metrics", func(writer http.ResponseWriter, _ *http.Request) {
		body := e.Metrics()
		writer.Header().Set("Content-Type", "text/plain; version=0.0.4")
		writer.Header().Set("Content-Length", fmt.Sprint(len(body)))
		writer.WriteHeader(http.StatusOK)
		_, _ = writer.Write(body)
	})
	return mux
}

func main() {
	listen := flag.String("listen", "0.0.0.0:9116", "address to serve /metrics on")
	query := flag.String("query", "", "fetch metrics from URL and print them, then exit")
	flag.Parse()

	if *query != "" {
		client := &http.Client{Timeout: 5 * time.Second}
		response, err := client.Get(*query)
		if err != nil {
			log.Printf("health-exporter: query %s: %v", *query, err)
			os.Exit(1)
		}
		defer response.Body.Close()
		body, err := io.ReadAll(response.Body)
		if err != nil {
			log.Printf("health-exporter: read %s: %v", *query, err)
			os.Exit(1)
		}
		if response.StatusCode != http.StatusOK {
			log.Printf("health-exporter: query %s: HTTP %d", *query, response.StatusCode)
			os.Exit(1)
		}
		os.Stdout.Write(body)
		if len(body) == 0 || body[len(body)-1] != '\n' {
			fmt.Println()
		}
		return
	}

	exporter := New()
	server := &http.Server{Addr: *listen, Handler: exporter.Handler(), ReadHeaderTimeout: 5 * time.Second}
	if err := server.ListenAndServe(); err != nil {
		log.Printf("health-exporter: %v", err)
		os.Exit(1)
	}
}
