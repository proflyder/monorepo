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
//
// `render`, `env get` and `oci` keep the argument shape of the Python scripts
// they replace (`render.py`, the deploy.sh `env_value` heredoc and
// `platform/ci/oci_image.py`), so the callers stay close to their old form.
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
	"ricardo.local/monorepo/platform/deploy/internal/envfile"
	"ricardo.local/monorepo/platform/deploy/internal/oci"
	"ricardo.local/monorepo/platform/deploy/internal/render"
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
`

func main() {
	if err := run(os.Args[1:], os.Stdout, os.Stderr); err != nil {
		if !errors.Is(err, flag.ErrHelp) {
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

// stringList collects a repeatable string flag.
type stringList []string

func (list *stringList) String() string {
	return strings.Join(*list, ",")
}

func (list *stringList) Set(value string) error {
	*list = append(*list, value)
	return nil
}
