package hermes

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"strings"
	"time"
)

// AssistSmokeClient performs one Assist turn through the in-cluster HA API and
// prints the JSON result line the rollout parses.
//
// It replaces the inline Python script that used to run in the smoke Job. The
// token is read from the mounted Secret file and is never printed. The exit
// status is 1 on every failure; the structured result lets the caller decide
// whether a 5xx is a transient Hermes warm-up.
func AssistSmokeClient(haURL, agent, text, tokenFile string, timeout time.Duration, stdout io.Writer) int {
	emit := func(payload map[string]interface{}) {
		encoded, _ := json.Marshal(payload)
		fmt.Fprintln(stdout, string(encoded))
	}
	token, err := os.ReadFile(tokenFile)
	if err != nil {
		emit(map[string]interface{}{"ok": false, "error": fmt.Sprintf("ReadError: %v", err)})
		return 1
	}
	body, err := json.Marshal(map[string]string{
		"text":     text,
		"language": "ru",
		"agent_id": agent,
	})
	if err != nil {
		emit(map[string]interface{}{"ok": false, "error": "EncodeError: " + err.Error()})
		return 1
	}
	request, err := http.NewRequest(http.MethodPost,
		strings.TrimRight(haURL, "/")+"/api/conversation/process", bytes.NewReader(body))
	if err != nil {
		emit(map[string]interface{}{"ok": false, "error": "RequestError: " + err.Error()})
		return 1
	}
	request.Header.Set("Authorization", "Bearer "+strings.TrimSpace(string(token)))
	request.Header.Set("Content-Type", "application/json")
	client := &http.Client{Timeout: timeout}
	response, err := client.Do(request)
	if err != nil {
		emit(map[string]interface{}{"ok": false, "error": fmt.Sprintf("URLError: %v", err)})
		return 1
	}
	defer response.Body.Close()
	raw, readErr := io.ReadAll(response.Body)
	if readErr != nil {
		emit(map[string]interface{}{"ok": false, "status": response.StatusCode, "error": "ReadError: " + readErr.Error()})
		return 1
	}
	if response.StatusCode != http.StatusOK {
		emit(map[string]interface{}{
			"ok":     false,
			"status": response.StatusCode,
			"error":  fmt.Sprintf("HTTPError: %d", response.StatusCode),
		})
		return 1
	}
	var data map[string]interface{}
	if err := json.Unmarshal(raw, &data); err != nil {
		emit(map[string]interface{}{"ok": false, "status": response.StatusCode, "error": "JSONError: " + err.Error()})
		return 1
	}
	reply, _ := data["response"].(map[string]interface{})
	speech := speechOf(reply)
	responseType := fmt.Sprint(reply["response_type"])
	if responseType == "<nil>" {
		responseType = ""
	}
	if responseType == "error" || strings.TrimSpace(speech) == "" {
		emit(map[string]interface{}{
			"ok":            false,
			"status":        response.StatusCode,
			"response_type": responseType,
			"speech":        speech,
			"data":          reply["data"],
		})
		return 1
	}
	emit(map[string]interface{}{"ok": true, "speech": speech})
	return 0
}

func speechOf(reply map[string]interface{}) string {
	speechBlock, _ := reply["speech"].(map[string]interface{})
	plain, _ := speechBlock["plain"].(map[string]interface{})
	value, _ := plain["speech"].(string)
	return value
}
