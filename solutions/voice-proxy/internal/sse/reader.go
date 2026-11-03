// Package sse implements the subset of Server-Sent Events the voice proxy
// needs: an incremental reader that joins multiline data, tolerates CRLF and
// forwards comment lines as keepalives, and a writer that flushes one frame at
// a time.
//
// The reader is a port of sse_events() in voice_proxy.py. Python iterates the
// HTTP response by lines and strips every trailing CR/LF; the protocol itself
// only requires one line ending, so the reader accepts both.
package sse

import (
	"bufio"
	"errors"
	"io"
	"strings"
	"unicode/utf8"
)

// Event is one decoded SSE record.
//
// Event is the "event:" field (default "message"). Keepalive records use the
// pseudo-event "keepalive" with empty Data, exactly like the Python reader.
type Event struct {
	Event string
	Data  string
}

// KeepaliveEvent is the pseudo event name for a comment line.
const KeepaliveEvent = "keepalive"

// Reader decodes SSE records from a stream.
type Reader struct {
	lines *bufio.Reader
	event string
	data  []string
	done  bool
}

// NewReader wraps r.
func NewReader(r io.Reader) *Reader {
	return &Reader{lines: bufio.NewReaderSize(r, 64<<10), event: "message"}
}

// ErrInvalidUTF8 reports a record that is not valid UTF-8; the Python reader
// raises UnicodeDecodeError on the same input.
var ErrInvalidUTF8 = errors.New("sse: data is not valid utf-8")

// Next returns the next record, or io.EOF at end of stream.
func (r *Reader) Next() (Event, error) {
	for {
		if r.done {
			return Event{}, io.EOF
		}
		line, err := r.readLine()
		if err != nil {
			if !errors.Is(err, io.EOF) {
				return Event{}, err
			}
			r.done = true
			if len(r.data) > 0 {
				ev := Event{Event: r.event, Data: strings.Join(r.data, "\n")}
				r.event, r.data = "message", nil
				return ev, nil
			}
			return Event{}, io.EOF
		}
		switch {
		case line == "":
			if len(r.data) > 0 {
				ev := Event{Event: r.event, Data: strings.Join(r.data, "\n")}
				r.event, r.data = "message", nil
				return ev, nil
			}
			r.event, r.data = "message", nil
		case strings.HasPrefix(line, "event:"):
			r.event = strings.TrimSpace(line[len("event:"):])
		case strings.HasPrefix(line, "data:"):
			r.data = append(r.data, strings.TrimPrefix(line[len("data:"):], " "))
		case strings.HasPrefix(line, ":"):
			return Event{Event: KeepaliveEvent}, nil
		}
	}
}

// readLine returns one line without its terminating newline. A final line
// without a newline is returned before io.EOF, matching Python's line iterator.
func (r *Reader) readLine() (string, error) {
	line, err := r.lines.ReadString('\n')
	if err != nil {
		if errors.Is(err, io.EOF) && line != "" {
			return finishLine(line)
		}
		return "", err
	}
	return finishLine(line)
}

// finishLine trims the line ending and validates UTF-8 per line, exactly like
// Python's raw.decode("utf-8").
func finishLine(line string) (string, error) {
	line = strings.TrimRight(line, "\r\n")
	if !utf8.ValidString(line) {
		return "", ErrInvalidUTF8
	}
	return line, nil
}
