// Command home-deploy is the single Go entry point for the home deployment
// tooling (decision 35: deploy tooling is Go).
//
// Subcommands:
//
//	home-deploy render [--unit U] [--env FILE] --out DIR [--set KEY=VALUE]... FILE...
//	home-deploy env get FILE KEY
//	home-deploy build-id [--revision REV] --fingerprint STR
//	home-deploy oci digest LAYOUT
//	home-deploy oci pack LAYOUT --ref REF --out FILE
//	home-deploy oci docker LAYOUT --ref REF --out FILE
//	home-deploy converge --kubectl K --namespace NS --name N --file F [--preserve-replicas] [--preserve-env]
//	home-deploy state record|mark|rollback --state-dir D ...
//	home-deploy preflight --ha-url URL --ha-token-file FILE ...
//	home-deploy runtime split --render F --private F --storage-out F --values-out F
//	home-deploy eso names --out F MANIFEST...
//	home-deploy secret opaque --name N --namespace NS --key K --from-env-file F --env KEY
//	home-deploy hermes rollout --values-file F --hermes-digest D --voice-proxy-digest D ...
//	home-deploy hermes render-config --home DIR [--private-values F] [--dry-run|--check]
//	home-deploy hermes assist-smoke
//
// `render`, `env get` and `oci` keep the argument shape of the Python scripts
// they replace (`render.py`, the deploy.sh `env_value` heredoc and
// `platform/ci/oci_image.py`); `converge`, `state` and `preflight` likewise keep
// the argv of `converge.py`, `rollback.py`/the deploy.sh markers and
// `preflight.py`, so the callers stay close to their old form.
// Output that callers capture (a rendered file's path lines, a value, a digest)
// goes to stdout; diagnostics go to stderr with exit status 1.
package main

import (
	"bytes"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"time"

	"ricardo.local/monorepo/platform/deploy/internal/buildid"
	"ricardo.local/monorepo/platform/deploy/internal/converge"
	"ricardo.local/monorepo/platform/deploy/internal/envfile"
	hermesdeploy "ricardo.local/monorepo/platform/deploy/internal/hermes"
	"ricardo.local/monorepo/platform/deploy/internal/hermesconfig"
	"ricardo.local/monorepo/platform/deploy/internal/kube"
	"ricardo.local/monorepo/platform/deploy/internal/manifests"
	"ricardo.local/monorepo/platform/deploy/internal/oci"
	"ricardo.local/monorepo/platform/deploy/internal/preflight"
	"ricardo.local/monorepo/platform/deploy/internal/render"
	"ricardo.local/monorepo/platform/deploy/internal/runtimevalues"
	"ricardo.local/monorepo/platform/deploy/internal/state"
)

const usage = `home-deploy <command> [options]

Commands:
  render   substitute ${KEY} placeholders in deploy templates
             --unit scheduler|currency-bot|mcp-gateway|wakegrant|ingress
             --env FILE   private KEY=VALUE file (optional)
             --out DIR    output directory
             --set K=V    override a placeholder (repeatable)
             --rollout-key K
                          fill K with a content fingerprint of the rendered unit
             FILE...      templates (an output file keeps the template name)
  env get FILE KEY
           print the value of KEY (empty output when absent)
  build-id --fingerprint STR [--revision REV]
           print <revision>-<first 12 hex of sha256(STR)>, or just the digest
           when --revision is empty
  oci digest LAYOUT
  oci pack LAYOUT --ref REF --out FILE
  oci docker LAYOUT --ref REF --out FILE
           inspect or pack a rules_oci image layout (byte-preserving)
  converge --kubectl K --namespace NS --name N --file F
           [--preserve-replicas] [--preserve-env] [--summary-file F]
           replace a live Deployment with the rendered manifest (prunes drift);
           --summary-file records created|unchanged|rolled-out
  state record --state-dir D --namespace NS --name N [--kubectl K]
  state mark --state-dir D --name N
  state rollback --state-dir D [--namespace NS] [--kubectl K]
           deploy rollback bookkeeping (.before.json / .absent / .applied)
  preflight --ha-url URL --ha-token-file FILE --scheduler-url URL
            --scheduler-token-file FILE [--expected-endpoint URL]
           read-only cutover checks for deploy.sh --execute
  runtime split --render F --private F --storage-out F --values-out F
            [--health-exporter-image DIGEST]
           split a Helm render into kubectl-applied storage and Helm values
  eso names --out F MANIFEST...
           list ExternalSecrets as <namespace>/<name>
  secret opaque --name N --namespace NS --key K --from-env-file F --env KEY
           print a one-key Opaque Secret JSON (bootstrap path)
  hermes rollout --values-file F --hermes-digest D --voice-proxy-digest D
            --exporter-digest D --smoke-image IMAGE --state-dir D [--force]
            [--simulate-smoke-failure] [--rollback-only]
           promote the Hermes/voice-proxy/exporter digests into the runtime release
  hermes render-config --home DIR [--private-values F] [--dry-run|--check]
           merge config.fragment.yaml into a live HERMES_HOME
  hermes assist-smoke
           one Assist turn through the in-cluster HA API (smoke Job client)
`

// errStatusReported means the command already printed its diagnostics and only
// the exit status has to change (preflight prints one line per failed check).
var errStatusReported = errors.New("status already reported")

// statusError changes the process exit status. A nil err means the command
// already printed its diagnostics.
type statusError struct {
	code int
	err  error
}

func (e *statusError) Error() string {
	if e.err == nil {
		return "status already reported"
	}
	return e.err.Error()
}

func (e *statusError) Unwrap() error { return e.err }

func main() {
	err := run(os.Args[1:], os.Stdout, os.Stderr)
	if err == nil {
		return
	}
	code := 1
	var status *statusError
	if errors.As(err, &status) {
		code = status.code
		if status.err != nil {
			fmt.Fprintln(os.Stderr, status.err)
		}
		os.Exit(code)
	}
	if !errors.Is(err, flag.ErrHelp) && !errors.Is(err, errStatusReported) {
		fmt.Fprintln(os.Stderr, err)
	}
	os.Exit(1)
}

func run(args []string, stdout, stderr io.Writer) error {
	if len(args) == 0 {
		fmt.Fprint(stderr, usage)
		return errors.New("missing command")
	}
	switch args[0] {
	case "-h", "--help", "help":
		fmt.Fprint(stdout, usage)
		return nil
	case "render":
		return runRender(args[1:], stdout, stderr)
	case "env":
		return runEnv(args[1:], stdout)
	case "build-id":
		return runBuildID(args[1:], stdout)
	case "oci":
		return runOCI(args[1:], stdout)
	case "converge":
		return runConverge(args[1:], stdout, stderr)
	case "state":
		return runState(args[1:], stdout, stderr)
	case "preflight":
		return runPreflight(args[1:], stdout, stderr)
	case "runtime":
		return runRuntime(args[1:], stdout)
	case "eso":
		return runESO(args[1:], stdout)
	case "secret":
		return runSecret(args[1:], stdout, stderr)
	case "hermes":
		return runHermes(args[1:], stdout, stderr)
	default:
		fmt.Fprint(stderr, usage)
		return fmt.Errorf("unknown command %q", args[0])
	}
}

func runRender(args []string, stdout, stderr io.Writer) error {
	flags := flag.NewFlagSet("render", flag.ContinueOnError)
	flags.SetOutput(stderr)
	unit := flags.String("unit", "scheduler", "deploy unit whose placeholder allowlist applies")
	envPath := flags.String("env", "", "private KEY=VALUE file")
	outDir := flags.String("out", "", "output directory")
	rolloutKey := flags.String("rollout-key", "", "placeholder filled with a content fingerprint of the rendered unit")
	sets := stringList{}
	flags.Var(&sets, "set", "KEY=VALUE override (repeatable)")
	if err := flags.Parse(args); err != nil {
		return err
	}
	files := flags.Args()
	if *outDir == "" {
		return errors.New("render: --out is required")
	}
	if len(files) == 0 {
		return errors.New("render: at least one template is required")
	}
	return render.Run(render.Options{
		EnvFile:    *envPath,
		Sets:       sets,
		OutDir:     *outDir,
		Files:      files,
		Unit:       *unit,
		RolloutKey: *rolloutKey,
		Stdout:     stdout,
	})
}

func runEnv(args []string, stdout io.Writer) error {
	if len(args) != 3 || args[0] != "get" {
		return errors.New("usage: home-deploy env get FILE KEY")
	}
	value, _, err := envfile.Lookup(args[1], args[2])
	if err != nil {
		return err
	}
	fmt.Fprintln(stdout, value)
	return nil
}

func runBuildID(args []string, stdout io.Writer) error {
	flags := flag.NewFlagSet("build-id", flag.ContinueOnError)
	flags.SetOutput(io.Discard)
	fingerprint := flags.String("fingerprint", "", "string that identifies the built image set")
	revision := flags.String("revision", "", "short git revision to prefix (optional)")
	if err := flags.Parse(args); err != nil {
		return err
	}
	if *fingerprint == "" {
		return errors.New("build-id: --fingerprint is required")
	}
	if *revision == "" {
		fmt.Fprintln(stdout, buildid.Fingerprint(*fingerprint))
		return nil
	}
	fmt.Fprintln(stdout, buildid.BuildID(*revision, *fingerprint))
	return nil
}

func runOCI(args []string, stdout io.Writer) error {
	if len(args) == 0 {
		return errors.New("usage: home-deploy oci digest|pack|docker ...")
	}
	switch args[0] {
	case "digest":
		if len(args) != 2 {
			return errors.New("usage: home-deploy oci digest LAYOUT")
		}
		digest, err := oci.Digest(args[1])
		if err != nil {
			return err
		}
		fmt.Fprintln(stdout, digest)
		return nil
	case "pack", "docker":
		layout, options, err := parsePackArgs(args[1:])
		if err != nil {
			return fmt.Errorf("oci %s: %w", args[0], err)
		}
		build := oci.Pack
		if args[0] == "docker" {
			build = oci.DockerArchive
		}
		digest, err := build(layout, options.ref, options.out)
		if err != nil {
			return err
		}
		fmt.Fprintln(stdout, digest)
		return nil
	default:
		return fmt.Errorf("unknown oci command %q (expected digest, pack or docker)", args[0])
	}
}

type packOptions struct {
	ref string
	out string
}

// parsePackArgs accepts the Python helper's argument order (LAYOUT first, then
// `--ref`/`--out`) as well as flags first; Go's flag package alone cannot
// handle a positional before options.
func parsePackArgs(args []string) (string, packOptions, error) {
	options := packOptions{}
	layout := ""
	for index := 0; index < len(args); index++ {
		argument := args[index]
		name, value, hasValue := strings.Cut(argument, "=")
		switch name {
		case "--ref", "-ref":
			if !hasValue {
				index++
				if index >= len(args) {
					return "", options, errors.New("--ref requires a value")
				}
				value = args[index]
			}
			options.ref = value
		case "--out", "-out":
			if !hasValue {
				index++
				if index >= len(args) {
					return "", options, errors.New("--out requires a value")
				}
				value = args[index]
			}
			options.out = value
		case "-h", "--help":
			return "", options, flag.ErrHelp
		default:
			if strings.HasPrefix(argument, "-") {
				return "", options, fmt.Errorf("unknown option %q", argument)
			}
			if layout != "" {
				return "", options, fmt.Errorf("unexpected extra argument %q", argument)
			}
			layout = argument
		}
	}
	if layout == "" {
		return "", options, errors.New("layout argument is required")
	}
	if options.ref == "" {
		return "", options, errors.New("--ref is required")
	}
	if options.out == "" {
		return "", options, errors.New("--out is required")
	}
	return layout, options, nil
}

func runConverge(args []string, stdout, stderr io.Writer) error {
	flags := flag.NewFlagSet("converge", flag.ContinueOnError)
	flags.SetOutput(stderr)
	kubectl := flags.String("kubectl", "kubectl", "kubectl binary")
	namespace := flags.String("namespace", "home-assistant", "Kubernetes namespace")
	name := flags.String("name", "", "Deployment name")
	file := flags.String("file", "", "rendered manifest")
	preserveReplicas := flags.Bool("preserve-replicas", false, "keep the live replica count")
	preserveEnv := flags.Bool("preserve-env", false, "keep the live container env/envFrom")
	summaryFile := flags.String("summary-file", "", "append '<name>\\t<status>' after applying")
	if err := flags.Parse(args); err != nil {
		return err
	}
	if *name == "" {
		return errors.New("converge: --name is required")
	}
	if *file == "" {
		return errors.New("converge: --file is required")
	}
	message, err := converge.Converge(converge.Options{
		Kubectl:          *kubectl,
		Namespace:        *namespace,
		Name:             *name,
		File:             *file,
		PreserveReplicas: *preserveReplicas,
		PreserveEnv:      *preserveEnv,
		SummaryFile:      *summaryFile,
	}, kube.Exec(*kubectl))
	if err != nil {
		return err
	}
	fmt.Fprintln(stdout, message)
	return nil
}

func runState(args []string, stdout, stderr io.Writer) error {
	if len(args) == 0 {
		return errors.New("usage: home-deploy state record|mark|rollback ...")
	}
	command := args[0]
	switch command {
	case "record":
		flags := flag.NewFlagSet("state record", flag.ContinueOnError)
		flags.SetOutput(stderr)
		stateDir := flags.String("state-dir", "", "rollback state directory")
		namespace := flags.String("namespace", "home-assistant", "Kubernetes namespace")
		name := flags.String("name", "", "Deployment name")
		kubectl := flags.String("kubectl", "kubectl", "kubectl binary")
		if err := flags.Parse(args[1:]); err != nil {
			return err
		}
		if *stateDir == "" || *name == "" {
			return errors.New("state record: --state-dir and --name are required")
		}
		return state.Record(*stateDir, *namespace, *name, kube.Exec(*kubectl))
	case "mark":
		flags := flag.NewFlagSet("state mark", flag.ContinueOnError)
		flags.SetOutput(stderr)
		stateDir := flags.String("state-dir", "", "rollback state directory")
		name := flags.String("name", "", "Deployment name")
		if err := flags.Parse(args[1:]); err != nil {
			return err
		}
		if *stateDir == "" || *name == "" {
			return errors.New("state mark: --state-dir and --name are required")
		}
		return state.Mark(*stateDir, *name)
	case "rollback":
		flags := flag.NewFlagSet("state rollback", flag.ContinueOnError)
		flags.SetOutput(stderr)
		stateDir := flags.String("state-dir", "", "rollback state directory")
		namespace := flags.String("namespace", "home-assistant", "Kubernetes namespace")
		kubectl := flags.String("kubectl", "kubectl", "kubectl binary")
		if err := flags.Parse(args[1:]); err != nil {
			return err
		}
		if *stateDir == "" {
			return errors.New("state rollback: --state-dir is required")
		}
		for _, message := range state.Rollback(*stateDir, *namespace, kube.Exec(*kubectl)) {
			fmt.Fprintln(stdout, message)
		}
		return nil
	default:
		return fmt.Errorf("unknown state command %q (expected record, mark or rollback)", command)
	}
}

func runPreflight(args []string, stdout, stderr io.Writer) error {
	flags := flag.NewFlagSet("preflight", flag.ContinueOnError)
	flags.SetOutput(stderr)
	haURL := flags.String("ha-url", "", "Home Assistant base URL")
	haTokenFile := flags.String("ha-token-file", "", "file holding the HA token")
	schedulerURL := flags.String("scheduler-url", "", "scheduler base URL")
	schedulerTokenFile := flags.String("scheduler-token-file", "", "file holding the scheduler token")
	expectedEndpoint := flags.String("expected-endpoint", "", "expected input_text.scheduler_endpoint")
	if err := flags.Parse(args); err != nil {
		return err
	}
	if *haURL == "" || *haTokenFile == "" || *schedulerURL == "" || *schedulerTokenFile == "" {
		return errors.New("preflight: --ha-url, --ha-token-file, --scheduler-url and --scheduler-token-file are required")
	}
	if _, err := os.Stat(*haTokenFile); err != nil {
		return fmt.Errorf("preflight: read HA token: %w", err)
	}
	if _, err := os.Stat(*schedulerTokenFile); err != nil {
		return fmt.Errorf("preflight: read scheduler token: %w", err)
	}
	status := preflight.Run(preflight.Options{
		HAURL:              *haURL,
		HATokenFile:        *haTokenFile,
		SchedulerURL:       *schedulerURL,
		SchedulerTokenFile: *schedulerTokenFile,
		ExpectedEndpoint:   *expectedEndpoint,
		Stdout:             stdout,
	})
	if status != 0 {
		return errStatusReported
	}
	return nil
}

// stringList collects a repeatable string flag.
type stringList []string

func (list *stringList) String() string {
	return strings.Join(*list, ",")
}

func (list *stringList) Set(value string) error {
	*list = append(*list, value)
	return nil
}

// repoRoot walks up from the working directory to the module root. It lets the
// binary default to the repository's tracked Hermes sources when it is run from
// a checkout without explicit paths.
func repoRoot() string {
	directory, err := os.Getwd()
	if err != nil {
		return ""
	}
	for {
		if _, err := os.Stat(filepath.Join(directory, "go.mod")); err == nil {
			return directory
		}
		parent := filepath.Dir(directory)
		if parent == directory {
			return ""
		}
		directory = parent
	}
}

func runRuntime(args []string, stdout io.Writer) error {
	if len(args) == 0 || args[0] != "split" {
		return errors.New("usage: home-deploy runtime split --render F --private F --storage-out F --values-out F")
	}
	flags := flag.NewFlagSet("runtime split", flag.ContinueOnError)
	flags.SetOutput(io.Discard)
	renderPath := flags.String("render", "", "helm template output")
	privatePath := flags.String("private", "", "private runtime values")
	storageOut := flags.String("storage-out", "", "storage documents output")
	valuesOut := flags.String("values-out", "", "workload values output")
	exporterImage := flags.String("health-exporter-image", "", "digest-pinned health exporter image")
	exporterWorkload := flags.String("health-exporter-workload", "runtime-health-exporter", "exporter workload key in the values")
	if err := flags.Parse(args[1:]); err != nil {
		return err
	}
	if *renderPath == "" || *privatePath == "" || *storageOut == "" || *valuesOut == "" {
		return errors.New("runtime split: --render, --private, --storage-out and --values-out are required")
	}
	storage, err := runtimevalues.SplitRender(*renderPath)
	if err != nil {
		return err
	}
	if err := runtimevalues.Write(*storageOut, storage); err != nil {
		return err
	}
	values, err := runtimevalues.WorkloadValues(*privatePath)
	if err != nil {
		return err
	}
	if *exporterImage != "" {
		values, err = runtimevalues.PinHealthExporter(values, *exporterImage, *exporterWorkload)
		if err != nil {
			return err
		}
	}
	encoded, err := runtimevalues.Marshal(values)
	if err != nil {
		return err
	}
	if err := runtimevalues.Write(*valuesOut, encoded); err != nil {
		return err
	}
	fmt.Fprintf(stdout, "wrote %s and %s\n", *storageOut, *valuesOut)
	return nil
}

func runESO(args []string, stdout io.Writer) error {
	if len(args) == 0 || args[0] != "names" {
		return errors.New("usage: home-deploy eso names --out F MANIFEST...")
	}
	flags := flag.NewFlagSet("eso names", flag.ContinueOnError)
	flags.SetOutput(io.Discard)
	out := flags.String("out", "", "output file")
	if err := flags.Parse(args[1:]); err != nil {
		return err
	}
	if *out == "" {
		return errors.New("eso names: --out is required")
	}
	if len(flags.Args()) == 0 {
		return errors.New("eso names: at least one manifest is required")
	}
	names, err := manifests.ExternalSecretNames(flags.Args())
	if err != nil {
		return err
	}
	if err := manifests.WriteNames(*out, names); err != nil {
		return err
	}
	fmt.Fprintf(stdout, "%d ExternalSecret(s)\n", len(names))
	return nil
}

func runSecret(args []string, stdout, stderr io.Writer) error {
	if len(args) == 0 || args[0] != "opaque" {
		return errors.New("usage: home-deploy secret opaque --name N --namespace NS --key K --from-env-file F --env KEY")
	}
	flags := flag.NewFlagSet("secret opaque", flag.ContinueOnError)
	flags.SetOutput(stderr)
	name := flags.String("name", "", "Secret name")
	namespace := flags.String("namespace", "", "Secret namespace")
	key := flags.String("key", "", "data key")
	fromEnvFile := flags.String("from-env-file", "", "private dotenv file")
	envKey := flags.String("env", "", "variable to read from the dotenv file")
	if err := flags.Parse(args[1:]); err != nil {
		return err
	}
	if *name == "" || *namespace == "" || *key == "" || *fromEnvFile == "" || *envKey == "" {
		return errors.New("secret opaque: --name, --namespace, --key, --from-env-file and --env are required")
	}
	document, err := manifests.OpaqueSecret(*name, *namespace, *key, *fromEnvFile, *envKey)
	if err != nil {
		return err
	}
	fmt.Fprintln(stdout, string(document))
	return nil
}

func runHermes(args []string, stdout, stderr io.Writer) error {
	if len(args) == 0 {
		return errors.New("usage: home-deploy hermes rollout|render-config|assist-smoke ...")
	}
	switch args[0] {
	case "rollout":
		return runHermesRollout(args[1:], stdout, stderr)
	case "render-config":
		return runHermesRenderConfig(args[1:], stdout, stderr)
	case "assist-smoke":
		return runHermesAssistSmoke(args[1:], stdout, stderr)
	default:
		return fmt.Errorf("unknown hermes command %q (expected rollout, render-config or assist-smoke)", args[0])
	}
}

func runHermesRollout(args []string, stdout, stderr io.Writer) error {
	flags := flag.NewFlagSet("hermes rollout", flag.ContinueOnError)
	flags.SetOutput(stderr)
	valuesFile := flags.String("values-file", "", "private runtime values file")
	hermesDigest := flags.String("hermes-digest", "", "pushed Hermes digest")
	proxyDigest := flags.String("voice-proxy-digest", "", "pushed voice-proxy digest")
	exporterDigest := flags.String("exporter-digest", "", "pushed health-exporter digest")
	smokeImage := flags.String("smoke-image", "", "digest-pinned home-deploy image for the smoke Job")
	release := flags.String("release", hermesdeploy.Release, "Helm release")
	deployment := flags.String("deployment", "hermes", "Hermes Deployment")
	exporterDeployment := flags.String("exporter-deployment", hermesdeploy.ExporterDeployment, "health exporter Deployment")
	namespace := flags.String("namespace", hermesdeploy.Namespace, "Kubernetes namespace")
	chart := flags.String("chart", "", "runtime chart directory")
	kubectl := flags.String("kubectl", "kubectl", "kubectl binary")
	helm := flags.String("helm", "helm", "helm binary")
	stateDir := flags.String("state-dir", "", "rollback state directory")
	timeout := flags.String("timeout", hermesdeploy.DefaultTimeout, "Helm timeout")
	force := flags.Bool("force", false, "upgrade even when the digests are unchanged")
	simulateFailure := flags.Bool("simulate-smoke-failure", false, "fail after the upgrade (rollback test)")
	rollbackOnly := flags.Bool("rollback-only", false, "roll back the recorded revision")
	if err := flags.Parse(args); err != nil {
		return err
	}
	if *chart == "" {
		if root := repoRoot(); root != "" {
			*chart = filepath.Join(root, "projects/home-assistant/deploy/chart")
		}
	}
	if *stateDir == "" {
		temporary, err := os.MkdirTemp("", "hermes-rollout-")
		if err != nil {
			return err
		}
		*stateDir = temporary
	}
	options := hermesdeploy.Options{
		ValuesFile:         *valuesFile,
		HermesDigest:       *hermesDigest,
		VoiceProxyDigest:   *proxyDigest,
		ExporterDigest:     *exporterDigest,
		SmokeImage:         *smokeImage,
		Release:            *release,
		Deployment:         *deployment,
		ExporterDeployment: *exporterDeployment,
		Namespace:          *namespace,
		Chart:              *chart,
		StateDir:           *stateDir,
		Timeout:            *timeout,
		Force:              *force,
		SimulateFailure:    *simulateFailure,
		RollbackOnly:       *rollbackOnly,
		Kubectl:            *kubectl,
		Helm:               *helm,
		Stdout:             stdout,
		Stderr:             stderr,
	}
	code, err := hermesdeploy.Run(hermesdeploy.Exec(), options)
	if err != nil || code != 0 {
		return &statusError{code: codeOrOne(code), err: err}
	}
	return nil
}

func codeOrOne(code int) int {
	if code == 0 {
		return 1
	}
	return code
}

func runHermesRenderConfig(args []string, stdout, stderr io.Writer) error {
	flags := flag.NewFlagSet("hermes render-config", flag.ContinueOnError)
	flags.SetOutput(stderr)
	home := flags.String("home", "", "HERMES_HOME")
	fragment := flags.String("fragment", "", "config fragment")
	privateValues := flags.String("private-values", "", "private values merged after the fragment")
	voicePrompt := flags.String("voice-prompt", "", "source for agent.personalities.voice")
	telegramPrompt := flags.String("telegram-prompt", "", "system prompt for the owner chat")
	contextFile := flags.String("context", "", "infra context installed as .hermes.md")
	soul := flags.String("soul", "", "identity file installed as SOUL.md")
	skills := flags.String("skills", "", "skill directories installed into <home>/skills")
	allowPlaceholders := flags.Bool("allow-placeholders", false, "allow TEST-NET/example placeholders (tests only)")
	dryRun := flags.Bool("dry-run", false, "print diffs, change nothing")
	check := flags.Bool("check", false, "like --dry-run, but exit 1 when a render is pending")
	if err := flags.Parse(args); err != nil {
		return err
	}
	if *home == "" {
		return errors.New("hermes render-config: --home is required")
	}
	if *dryRun && *check {
		return errors.New("hermes render-config: --dry-run and --check are mutually exclusive")
	}
	sourceDir := ""
	if root := repoRoot(); root != "" {
		sourceDir = filepath.Join(root, "projects/home-assistant/hermes")
	}
	defaultPath := func(value, name string) string {
		if value != "" || sourceDir == "" {
			return value
		}
		return filepath.Join(sourceDir, name)
	}
	options := hermesconfig.Options{
		Home:              *home,
		Fragment:          defaultPath(*fragment, "config.fragment.yaml"),
		Context:           defaultPath(*contextFile, "HERMES.md"),
		Soul:              defaultPath(*soul, "SOUL.md"),
		VoicePrompt:       defaultPath(*voicePrompt, "voice_prompt.txt"),
		TelegramPrompt:    defaultPath(*telegramPrompt, "telegram_prompt.txt"),
		Skills:            defaultPath(*skills, "skills"),
		PrivateValues:     *privateValues,
		AllowPlaceholders: *allowPlaceholders,
	}
	var buffer bytes.Buffer
	code, err := hermesconfig.Run(options, *dryRun, *check, &buffer)
	if buffer.Len() > 0 {
		fmt.Fprint(stdout, buffer.String())
	}
	if err != nil {
		return err
	}
	if code != 0 {
		return &statusError{code: code}
	}
	return nil
}

func runHermesAssistSmoke(args []string, stdout, stderr io.Writer) error {
	flags := flag.NewFlagSet("hermes assist-smoke", flag.ContinueOnError)
	flags.SetOutput(stderr)
	haURL := flags.String("ha-url", hermesdeploy.HAURL, "Home Assistant base URL")
	agent := flags.String("agent", hermesdeploy.HAAgent, "conversation agent id")
	text := flags.String("text", hermesdeploy.SmokeText, "Assist text")
	tokenFile := flags.String("token-file", "/run/secrets/ha/token", "mounted HA token file")
	timeout := flags.Duration("timeout", 150*time.Second, "request timeout")
	if err := flags.Parse(args); err != nil {
		return err
	}
	code := hermesdeploy.AssistSmokeClient(*haURL, *agent, *text, *tokenFile, *timeout, stdout)
	if code != 0 {
		return &statusError{code: code}
	}
	return nil
}
