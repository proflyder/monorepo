package scheduler

import (
	"bytes"
	"encoding/json"
	"fmt"
	"strconv"
	"strings"
)

// protoInt64 decodes a protobuf-JSON int64: a decimal string per the standard
// mapping, tolerating a bare number from hand-written fakes or older servers.
type protoInt64 int64

func (p *protoInt64) UnmarshalJSON(raw []byte) error {
	raw = bytes.TrimSpace(raw)
	if len(raw) == 0 || bytes.Equal(raw, []byte("null")) {
		*p = 0
		return nil
	}
	text := string(raw)
	if raw[0] == '"' {
		var s string
		if err := json.Unmarshal(raw, &s); err != nil {
			return err
		}
		text = strings.TrimSpace(s)
		if text == "" {
			*p = 0
			return nil
		}
	}
	v, err := strconv.ParseInt(text, 10, 64)
	if err != nil {
		return fmt.Errorf("invalid int64 %q", text)
	}
	*p = protoInt64(v)
	return nil
}

// protoInt32 is the same for int32 fields.
type protoInt32 int32

func (p *protoInt32) UnmarshalJSON(raw []byte) error {
	var wide protoInt64
	if err := wide.UnmarshalJSON(raw); err != nil {
		return err
	}
	*p = protoInt32(wide)
	return nil
}

// protoEnum decodes an enum name ("SOURCE_KIND_ALARM") or its numeric value.
type protoEnum string

func (p *protoEnum) UnmarshalJSON(raw []byte) error {
	raw = bytes.TrimSpace(raw)
	if len(raw) == 0 || bytes.Equal(raw, []byte("null")) {
		*p = ""
		return nil
	}
	if raw[0] == '"' {
		var s string
		if err := json.Unmarshal(raw, &s); err != nil {
			return err
		}
		*p = protoEnum(s)
		return nil
	}
	*p = protoEnum(string(raw))
	return nil
}

// enumValue normalizes a decoded enum to a lowercase suffix. Numeric values
// are mapped through numbers; unknown numbers keep their decimal form so the
// caller can report them without executing anything.
func enumValue(v protoEnum, prefix string, numbers map[int]string) string {
	text := strings.TrimSpace(string(v))
	if text == "" {
		return ""
	}
	if n, err := strconv.Atoi(text); err == nil {
		if name, ok := numbers[n]; ok {
			return name
		}
		return "enum-" + text
	}
	text = strings.TrimPrefix(text, prefix)
	return strings.ToLower(text)
}

func sourceKindOf(v protoEnum) SourceKind {
	name := enumValue(v, "SOURCE_KIND_", map[int]string{
		0: string(SourceKindUnspecified),
		1: string(SourceKindAlarm),
		2: string(SourceKindReminder),
		3: string(SourceKindTimer),
		4: string(SourceKindScenario),
		5: string(SourceKindTask),
	})
	if name == "unspecified" {
		return SourceKindUnspecified
	}
	return SourceKind(name)
}

func actionOf(v protoEnum) Action {
	name := strings.ToUpper(enumValue(v, "EVENT_ACTION_", map[int]string{
		0: string(ActionFire), // legacy rows: unspecified means FIRE
		1: string(ActionFire),
		2: string(ActionStop),
	}))
	switch name {
	case "", string(ActionFire):
		return ActionFire
	case string(ActionStop):
		return ActionStop
	default:
		return Action(name)
	}
}
