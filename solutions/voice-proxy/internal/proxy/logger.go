package proxy

import (
	"fmt"
	"io"
	"sync"
)

// Logger is the narrow logging surface of the proxy.
//
// Logs carry structure only: route outcomes, counts, timings and the stream
// flag, never recognised speech. The deploy smoke test greps for an uppercase
// ERROR token, so errors must keep one; the other lines stay in the
// "voice-proxy: <message>" shape OPERATIONS greps.
type Logger interface {
	Infof(format string, args ...any)
	Errorf(format string, args ...any)
}

// NopLogger discards everything; it keeps tests quiet.
type NopLogger struct{}

// Infof implements Logger.
func (NopLogger) Infof(string, ...any) {}

// Errorf implements Logger.
func (NopLogger) Errorf(string, ...any) {}

// StderrLogger writes "voice-proxy: ..." lines, like Handler.log_message in
// voice_proxy.py. Errors are prefixed with an uppercase ERROR level so the
// rollout log scan still catches them.
type StderrLogger struct {
	mu  sync.Mutex
	out io.Writer
}

// NewStderrLogger writes to out (normally os.Stderr).
func NewStderrLogger(out io.Writer) *StderrLogger { return &StderrLogger{out: out} }

// Infof implements Logger.
func (l *StderrLogger) Infof(format string, args ...any) {
	l.write("", format, args)
}

// Errorf implements Logger.
func (l *StderrLogger) Errorf(format string, args ...any) {
	l.write("ERROR ", format, args)
}

func (l *StderrLogger) write(level, format string, args []any) {
	l.mu.Lock()
	defer l.mu.Unlock()
	_, _ = fmt.Fprintf(l.out, "voice-proxy: %s%s\n", level, fmt.Sprintf(format, args...))
}
