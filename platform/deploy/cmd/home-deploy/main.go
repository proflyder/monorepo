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
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"strings"

	"ricardo.local/monorepo/platform/deploy/internal/buildid"
	"ricardo.local/monorepo/platform/deploy/internal/converge"
	"ricardo.local/monorepo/platform/deploy/internal/envfile"
	"ricardo.local/monorepo/platform/deploy/internal/kube"
	"ricardo.local/monorepo/platform/deploy/internal/oci"
	"ricardo.local/monorepo/platform/deploy/internal/preflight"
	"ricardo.local/monorepo/platform/deploy/internal/render"
	"ricardo.local/monorepo/platform/deploy/internal/state"
)

const usage = `home-deploy <command> [options]

Commands:
  render   substitute ${KEY} placeholders in deploy templates
             --unit scheduler|currency-bot|mcp-gateway|wakegrant|ingress
             --env FILE   private KEY=VALUE file (optional)
             --out DIR    output directory
             --set K=V    override a placeholder (repeatable)
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
           [--preserve-replicas] [--preserve-env]
           replace a live Deployment with the rendered manifest (prunes drift)
  state record --state-dir D --namespace NS --name N [--kubectl K]
  state mark --state-dir D --name N
  state rollback --state-dir D [--namespace NS] [--kubectl K]
           deploy rollback bookkeeping (.before.json / .absent / .applied)
  preflight --ha-url URL --ha-token-file FILE --scheduler-url URL
            --scheduler-token-file FILE [--expected-endpoint URL]
           read-only cutover checks for deploy.sh --execute
`

// errStatusReported means the command already printed its diagnostics and only
// the exit status has to change (preflight prints one line per failed check).
var errStatusReported = errors.New("status already reported")

func main() {
	if err := run(os.Args[1:], os.Stdout, os.Stderr); err != nil {
		if !errors.Is(err, flag.ErrHelp) && !errors.Is(err, errStatusReported) {
			fmt.Fprintln(os.Stderr, err)
		}
		os.Exit(1)
	}
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
		EnvFile: *envPath,
		Sets:    sets,
		OutDir:  *outDir,
		Files:   files,
		Unit:    *unit,
		Stdout:  stdout,
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
