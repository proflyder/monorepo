// Command google_services generates Android string resources from
// google-services.json.
//
// The Firebase Gradle plugin normally turns `google-services.json` into
// `res/values/google_services.xml` (and a few other resources) at build time.
// The Home Android app is built with Bazel only, so this tool does the same
// subset of that transformation:
//
//   - google_app_id                  <- mobilesdk_app_id
//   - gcm_defaultSenderId            <- project_info.project_number
//   - google_api_key                 <- client.api_key.current_key
//   - google_crash_reporting_api_key <- client.api_key.current_key
//   - google_storage_bucket          <- project_info.storage_bucket
//   - project_id                     <- project_info.project_id
//
// It also emits the Crashlytics build id that the Crashlytics Gradle plugin
// normally writes (`com.google.firebase.crashlytics.mapping_file_id`). The
// Crashlytics SDK only needs this string at init: `AppData` reads
// `com.google.firebase.crashlytics.mapping_file_id`, falling back to the legacy
// `com.crashlytics.android.build_id`, and refuses to start when both are absent.
// The value must be stable for one build and change when the build changes, so
// it is derived from the app id, version name, and version code (32 hex chars).
//
// The JSON is optional in the repository: without it the output is a valid,
// empty values file and a warning goes to stderr, so CI and clean checkouts keep
// building. Firebase then stays uninitialized.
package main

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
)

var firebaseResources = []string{
	"google_app_id",
	"gcm_defaultSenderId",
	"google_api_key",
	"google_crash_reporting_api_key",
	"google_storage_bucket",
	"project_id",
}

const crashlyticsBuildIDResource = "com.google.firebase.crashlytics.mapping_file_id"

const emptyValues = `<?xml version="1.0" encoding="utf-8"?>
<resources>
    <!-- google-services.json is not linked; Firebase stays uninitialized. -->
</resources>
`

// ConfigError means google-services.json does not describe the requested package.
type ConfigError struct {
	Package string
}

func (e *ConfigError) Error() string {
	return fmt.Sprintf("google-services.json has no client for package '%s'", e.Package)
}

func asMap(value any) map[string]any {
	result, _ := value.(map[string]any)
	return result
}

func asString(value any) string {
	text, _ := value.(string)
	return text
}

func clientForPackage(config map[string]any, packageName string) (map[string]any, error) {
	clients, _ := config["client"].([]any)
	for _, item := range clients {
		client := asMap(item)
		info := asMap(client["client_info"])
		android := asMap(info["android_client_info"])
		if asString(android["package_name"]) == packageName {
			return client, nil
		}
	}
	return nil, &ConfigError{Package: packageName}
}

func buildID(appID, versionName, versionCode string) string {
	material := strings.Join([]string{appID, versionName, versionCode}, "\n")
	sum := sha256.Sum256([]byte(material))
	return hex.EncodeToString(sum[:])[:32]
}

// escapeXML mirrors xml.sax.saxutils.escape(value, {'"': "&quot;"}).
func escapeXML(value string) string {
	var b strings.Builder
	for _, r := range value {
		switch r {
		case '&':
			b.WriteString("&amp;")
		case '>':
			b.WriteString("&gt;")
		case '<':
			b.WriteString("&lt;")
		case '"':
			b.WriteString("&quot;")
		default:
			b.WriteRune(r)
		}
	}
	return b.String()
}

func stringElement(name, value string) string {
	return fmt.Sprintf(`    <string name="%s" translatable="false">%s</string>`, name, escapeXML(value))
}

// Generate returns the values XML for one package. An absent or empty input
// yields an empty values file; a present input without the package is an error.
func Generate(inputPath, packageName, versionName, versionCode string) (string, error) {
	if inputPath == "" {
		return emptyValues, nil
	}
	info, err := os.Stat(inputPath)
	if err != nil || info.Size() == 0 {
		return emptyValues, nil
	}

	payload, err := os.ReadFile(inputPath)
	if err != nil {
		return "", err
	}
	var config map[string]any
	if err := json.Unmarshal(payload, &config); err != nil {
		return "", err
	}

	projectInfo := asMap(config["project_info"])
	client, err := clientForPackage(config, packageName)
	if err != nil {
		return "", err
	}
	apiKey := ""
	if entries, ok := client["api_key"].([]any); ok && len(entries) > 0 {
		apiKey = asString(asMap(entries[0])["current_key"])
	}
	appID := asString(asMap(client["client_info"])["mobilesdk_app_id"])

	values := map[string]string{
		"google_app_id":                  appID,
		"gcm_defaultSenderId":            asString(projectInfo["project_number"]),
		"google_api_key":                 apiKey,
		"google_crash_reporting_api_key": apiKey,
		"google_storage_bucket":          asString(projectInfo["storage_bucket"]),
		"project_id":                     asString(projectInfo["project_id"]),
	}

	lines := []string{`<?xml version="1.0" encoding="utf-8"?>`, "<resources>"}
	for _, name := range firebaseResources {
		lines = append(lines, stringElement(name, values[name]))
	}
	lines = append(lines, stringElement(crashlyticsBuildIDResource, buildID(appID, versionName, versionCode)))
	lines = append(lines, "</resources>")
	return strings.Join(lines, "\n") + "\n", nil
}

func run(args []string, stdout, stderr io.Writer) int {
	flags := flag.NewFlagSet("google_services", flag.ContinueOnError)
	flags.SetOutput(stderr)
	input := flags.String("input", "", "path to google-services.json")
	packageName := flags.String("package", "", "Android application id")
	versionName := flags.String("version-name", "0.1.0", "version name")
	versionCode := flags.String("version-code", "1", "version code")
	output := flags.String("output", "", "output file")
	if err := flags.Parse(args); err != nil {
		return 2
	}
	if *packageName == "" || *output == "" {
		fmt.Fprintln(stderr, "google_services: --package and --output are required")
		return 2
	}

	if *input == "" {
		fmt.Fprint(stderr, "WARNING: google-services.json is not linked; generating an empty "+
			"values file and leaving Firebase uninitialized. Link "+
			"platform/android/keys/google-services.json to enable Crashlytics.\n")
	} else if _, err := os.Stat(*input); err != nil {
		fmt.Fprint(stderr, "WARNING: google-services.json is not linked; generating an empty "+
			"values file and leaving Firebase uninitialized. Link "+
			"platform/android/keys/google-services.json to enable Crashlytics.\n")
	}

	xml, err := Generate(*input, *packageName, *versionName, *versionCode)
	if err != nil {
		// ConfigError and JSON/IO failures all exit non-zero with a single
		// diagnostic line, matching the previous runner.
		fmt.Fprintf(stderr, "ERROR: %s\n", err)
		return 1
	}

	if directory := filepath.Dir(*output); directory != "." && directory != "" {
		if err := os.MkdirAll(directory, 0o755); err != nil {
			fmt.Fprintf(stderr, "ERROR: %s\n", err)
			return 1
		}
	}
	if err := os.WriteFile(*output, []byte(xml), 0o644); err != nil {
		fmt.Fprintf(stderr, "ERROR: %s\n", err)
		return 1
	}
	return 0
}

func main() {
	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}
