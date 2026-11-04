package oci

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"unicode/utf16"
)

// jsonValue is a small ordered JSON representation.
//
// The OCI helpers must re-serialize `index.json` exactly like Python's
// `json.dumps(index, indent=2)`, including the original key order. Go's
// `encoding/json` sorts map keys and escapes HTML characters, so neither the
// digest nor the bytes of a packed archive would survive a round trip through
// `map[string]any`. This type keeps insertion order and re-emits Python's
// formatting.
type jsonValue struct {
	kind jsonKind
	obj  *orderedObject
	arr  []*jsonValue
	str  string
	num  string
	flag bool
}

type jsonKind int

const (
	kindNull jsonKind = iota
	kindBool
	kindNumber
	kindString
	kindArray
	kindObject
)

type orderedObject struct {
	keys []string
	vals map[string]*jsonValue
}

func newJSONObject() *orderedObject {
	return &orderedObject{vals: make(map[string]*jsonValue)}
}

// set inserts or replaces a key. A replaced key keeps its original position,
// like a Python dict.
func (o *orderedObject) set(key string, value *jsonValue) {
	if _, exists := o.vals[key]; !exists {
		o.keys = append(o.keys, key)
	}
	o.vals[key] = value
}

func (o *orderedObject) get(key string) (*jsonValue, bool) {
	value, ok := o.vals[key]
	return value, ok
}

// decodeJSON parses data preserving object key order and numeric literals.
func decodeJSON(data []byte) (*jsonValue, error) {
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.UseNumber()
	value, err := decodeJSONValue(decoder)
	if err != nil {
		return nil, err
	}
	// Reject trailing content the way json.loads does.
	if _, err := decoder.Token(); err == nil {
		return nil, errors.New("trailing data after the JSON document")
	} else if !errors.Is(err, io.EOF) {
		return nil, err
	}
	return value, nil
}

func decodeJSONValue(decoder *json.Decoder) (*jsonValue, error) {
	token, err := decoder.Token()
	if err != nil {
		return nil, err
	}
	switch value := token.(type) {
	case json.Delim:
		switch value {
		case '{':
			object := newJSONObject()
			for decoder.More() {
				keyToken, err := decoder.Token()
				if err != nil {
					return nil, err
				}
				key, ok := keyToken.(string)
				if !ok {
					return nil, fmt.Errorf("object key is not a string")
				}
				item, err := decodeJSONValue(decoder)
				if err != nil {
					return nil, err
				}
				object.set(key, item)
			}
			if _, err := decoder.Token(); err != nil {
				return nil, err
			}
			return &jsonValue{kind: kindObject, obj: object}, nil
		case '[':
			items := make([]*jsonValue, 0)
			for decoder.More() {
				item, err := decodeJSONValue(decoder)
				if err != nil {
					return nil, err
				}
				items = append(items, item)
			}
			if _, err := decoder.Token(); err != nil {
				return nil, err
			}
			return &jsonValue{kind: kindArray, arr: items}, nil
		default:
			return nil, fmt.Errorf("unexpected delimiter %q", value)
		}
	case string:
		return &jsonValue{kind: kindString, str: value}, nil
	case json.Number:
		return &jsonValue{kind: kindNumber, num: value.String()}, nil
	case bool:
		return &jsonValue{kind: kindBool, flag: value}, nil
	case nil:
		return &jsonValue{kind: kindNull}, nil
	default:
		return nil, fmt.Errorf("unexpected JSON token %T", token)
	}
}

// dumpJSONIndent renders the value like `json.dumps(value, indent=2)`: two
// spaces per level, no trailing newline, ASCII-only output.
func dumpJSONIndent(value *jsonValue) []byte {
	buffer := make([]byte, 0, 256)
	buffer = appendJSON(buffer, value, 2, 0)
	return buffer
}

// dumpJSONCompact renders the value like `json.dumps(value)`: Python's default
// separators (", " and ": ") and a single line.
func dumpJSONCompact(value *jsonValue) []byte {
	buffer := make([]byte, 0, 128)
	buffer = appendJSON(buffer, value, 0, 0)
	return buffer
}

func appendJSON(buffer []byte, value *jsonValue, indent, level int) []byte {
	switch value.kind {
	case kindNull:
		return append(buffer, "null"...)
	case kindBool:
		if value.flag {
			return append(buffer, "true"...)
		}
		return append(buffer, "false"...)
	case kindNumber:
		return append(buffer, value.num...)
	case kindString:
		return appendPythonString(buffer, value.str)
	case kindArray:
		if len(value.arr) == 0 {
			return append(buffer, "[]"...)
		}
		buffer = append(buffer, '[')
		for index, item := range value.arr {
			if index > 0 {
				buffer = appendItemSeparator(buffer, indent, level+1)
			} else if indent > 0 {
				buffer = appendNewlineIndent(buffer, indent, level+1)
			}
			buffer = appendJSON(buffer, item, indent, level+1)
		}
		if indent > 0 {
			buffer = appendNewlineIndent(buffer, indent, level)
		}
		return append(buffer, ']')
	case kindObject:
		if value.obj == nil || len(value.obj.keys) == 0 {
			return append(buffer, "{}"...)
		}
		buffer = append(buffer, '{')
		for index, key := range value.obj.keys {
			if index > 0 {
				buffer = appendItemSeparator(buffer, indent, level+1)
			} else if indent > 0 {
				buffer = appendNewlineIndent(buffer, indent, level+1)
			}
			buffer = appendPythonString(buffer, key)
			buffer = append(buffer, ':', ' ')
			buffer = appendJSON(buffer, value.obj.vals[key], indent, level+1)
		}
		if indent > 0 {
			buffer = appendNewlineIndent(buffer, indent, level)
		}
		return append(buffer, '}')
	}
	return buffer
}

func appendItemSeparator(buffer []byte, indent, level int) []byte {
	buffer = append(buffer, ',')
	if indent > 0 {
		return appendNewlineIndent(buffer, indent, level)
	}
	return append(buffer, ' ')
}

func appendNewlineIndent(buffer []byte, indent, level int) []byte {
	buffer = append(buffer, '\n')
	for count := 0; count < indent*level; count++ {
		buffer = append(buffer, ' ')
	}
	return buffer
}

// appendPythonString escapes a string exactly like Python's `json.dumps` with
// its default `ensure_ascii=True`: control characters use short escapes where
// Python has one, everything outside printable ASCII becomes `\uXXXX`
// (lower-case hex, surrogate pairs for non-BMP runes), and `<`, `>`, `&` stay
// literal (Go's encoder would HTML-escape them).
func appendPythonString(buffer []byte, value string) []byte {
	buffer = append(buffer, '"')
	for _, runeValue := range value {
		switch runeValue {
		case '"':
			buffer = append(buffer, '\\', '"')
		case '\\':
			buffer = append(buffer, '\\', '\\')
		case '\n':
			buffer = append(buffer, '\\', 'n')
		case '\t':
			buffer = append(buffer, '\\', 't')
		case '\r':
			buffer = append(buffer, '\\', 'r')
		case '\b':
			buffer = append(buffer, '\\', 'b')
		case '\f':
			buffer = append(buffer, '\\', 'f')
		default:
			if runeValue >= 0x20 && runeValue <= 0x7e {
				buffer = append(buffer, byte(runeValue))
				continue
			}
			if runeValue > 0xffff {
				high, low := utf16.EncodeRune(runeValue)
				buffer = appendUnicodeEscape(buffer, high)
				buffer = appendUnicodeEscape(buffer, low)
				continue
			}
			buffer = appendUnicodeEscape(buffer, runeValue)
		}
	}
	return append(buffer, '"')
}

func appendUnicodeEscape(buffer []byte, runeValue rune) []byte {
	const hexDigits = "0123456789abcdef"
	buffer = append(buffer, '\\', 'u')
	buffer = append(buffer,
		hexDigits[(runeValue>>12)&0xf],
		hexDigits[(runeValue>>8)&0xf],
		hexDigits[(runeValue>>4)&0xf],
		hexDigits[runeValue&0xf],
	)
	return buffer
}

// fieldString reads a string field, returning ok=false for a missing field or a
// non-string value.
func fieldString(object *orderedObject, key string) (string, bool) {
	value, found := object.get(key)
	if !found || value.kind != kindString {
		return "", false
	}
	return value.str, true
}

func jsonArrayOf(object *orderedObject, key string) []*jsonValue {
	value, found := object.get(key)
	if !found || value.kind != kindArray {
		return nil
	}
	return value.arr
}
