package proxy

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"strconv"
)

// decodeJSON decodes exactly one JSON value, keeping numbers as their original
// text so a re-encoded response never reformats an upstream timestamp.
func decodeJSON(data []byte) (any, error) {
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.UseNumber()
	var value any
	if err := dec.Decode(&value); err != nil {
		return nil, err
	}
	var extra any
	if err := dec.Decode(&extra); !errors.Is(err, io.EOF) {
		return nil, errors.New("trailing data after JSON value")
	}
	return value, nil
}

// encodeJSON is json.dumps(..., ensure_ascii=False): no HTML escaping, no
// trailing newline, UTF-8 output.
func encodeJSON(value any) ([]byte, error) {
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	if err := enc.Encode(value); err != nil {
		return nil, err
	}
	return bytes.TrimSuffix(buf.Bytes(), []byte("\n")), nil
}

// intValue reads a JSON number used as a choice index, mirroring
// choice.get("index", 0) which defaults to 0 when the field is absent.
func intValue(value any, fallback int) int {
	switch number := value.(type) {
	case json.Number:
		if parsed, err := strconv.Atoi(number.String()); err == nil {
			return parsed
		}
		if parsed, err := number.Float64(); err == nil {
			return int(parsed)
		}
	case float64:
		return int(number)
	case int:
		return number
	}
	return fallback
}
