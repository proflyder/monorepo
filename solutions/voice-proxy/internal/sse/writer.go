package sse

import (
	"bytes"
	"encoding/json"
	"io"
)

// Writer emits SSE frames to a response body.
//
// Every frame is written as "data: <payload>\n\n" and flushed immediately, so
// Piper can start speaking the first sentence before the upstream turn ends.
type Writer struct {
	w   io.Writer
	buf bytes.Buffer
}

// NewWriter wraps w. The caller owns flushing: HTTP handlers pass an
// http.Flusher as w when they need per-frame delivery.
func NewWriter(w io.Writer) *Writer { return &Writer{w: w} }

// WriteData writes one data frame. A string payload is written verbatim
// (used for the "[DONE]" sentinel); anything else is JSON-encoded without
// ASCII escaping, so non-Latin speech stays readable.
func (w *Writer) WriteData(payload any) error {
	w.buf.Reset()
	w.buf.WriteString("data: ")
	switch value := payload.(type) {
	case string:
		w.buf.WriteString(value)
	case []byte:
		w.buf.Write(value)
	default:
		enc := json.NewEncoder(&w.buf)
		enc.SetEscapeHTML(false)
		if err := enc.Encode(value); err != nil {
			return err
		}
		// Encode appends a newline; the frame terminator adds another.
		w.buf.Truncate(w.buf.Len() - 1)
	}
	w.buf.WriteString("\n\n")
	_, err := w.w.Write(w.buf.Bytes())
	return err
}

// WriteKeepalive writes a comment frame. HA notices a dead client while the
// upstream model is still thinking only because this frame is flushed.
func (w *Writer) WriteKeepalive() error {
	_, err := io.WriteString(w.w, ": keepalive\n\n")
	return err
}
