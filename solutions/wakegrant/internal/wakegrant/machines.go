package wakegrant

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	"os"
	"regexp"
	"sort"
	"strings"
)

// MachinePattern is the machine identifier grammar from PROTOCOL.md.
var MachinePattern = regexp.MustCompile(`^[a-z0-9-]{1,32}$`)

// KeySize is the required per-machine HMAC key length.
const KeySize = 32

// Keys maps a machine id to its 32-byte HMAC key.
type Keys map[string][]byte

// LoadMachinesFile reads the machine/key map from a mounted Secret file.
func LoadMachinesFile(path string) (Keys, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("cannot read machines file")
	}
	return ParseMachines(data)
}

// ParseMachines accepts either a JSON object {"machine":"base64key"} or
// newline-separated "machine=base64key" / "machine base64key" records. Blank
// lines and lines starting with '#' are ignored. Every key must decode to
// exactly 32 bytes, so a truncated Secret fails before the service starts.
func ParseMachines(data []byte) (Keys, error) {
	keys := Keys{}
	trimmed := strings.TrimSpace(string(data))
	if trimmed == "" {
		return nil, fmt.Errorf("machines file is empty")
	}
	if strings.HasPrefix(trimmed, "{") {
		var raw map[string]string
		if err := json.Unmarshal([]byte(trimmed), &raw); err != nil {
			return nil, fmt.Errorf("machines file is not valid JSON: %w", err)
		}
		for machine, encoded := range raw {
			if err := addKey(keys, machine, encoded); err != nil {
				return nil, err
			}
		}
	} else {
		for lineno, line := range strings.Split(trimmed, "\n") {
			line = strings.TrimSpace(line)
			if line == "" || strings.HasPrefix(line, "#") {
				continue
			}
			machine, encoded, ok := splitRecord(line)
			if !ok {
				return nil, fmt.Errorf("machines file line %d: expected machine=base64key", lineno+1)
			}
			if err := addKey(keys, machine, encoded); err != nil {
				return nil, err
			}
		}
	}
	if len(keys) == 0 {
		return nil, fmt.Errorf("machines file contains no machines")
	}
	return keys, nil
}

func splitRecord(line string) (string, string, bool) {
	fields := strings.Fields(line)
	if len(fields) == 2 && MachinePattern.MatchString(fields[0]) {
		return fields[0], fields[1], true
	}
	if i := strings.IndexByte(line, '='); i >= 0 {
		return strings.TrimSpace(line[:i]), strings.TrimSpace(line[i+1:]), true
	}
	return "", "", false
}

func addKey(keys Keys, machine, encoded string) error {
	if !MachinePattern.MatchString(machine) {
		return fmt.Errorf("machines file: invalid machine id %q", machine)
	}
	key, err := base64.StdEncoding.DecodeString(strings.TrimSpace(encoded))
	if err != nil {
		return fmt.Errorf("machines file: machine %q key is not valid base64", machine)
	}
	if len(key) != KeySize {
		return fmt.Errorf("machines file: machine %q key must be %d bytes", machine, KeySize)
	}
	if _, exists := keys[machine]; exists {
		return fmt.Errorf("machines file: duplicate machine %q", machine)
	}
	keys[machine] = key
	return nil
}

// Names returns the sorted machine ids (safe for logs; never the keys).
func (k Keys) Names() []string {
	names := make([]string, 0, len(k))
	for name := range k {
		names = append(names, name)
	}
	sort.Strings(names)
	return names
}
