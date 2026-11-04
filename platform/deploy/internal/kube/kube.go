// Package kube runs kubectl as a subprocess.
//
// The deploy helpers keep the exact argv the Python scripts used, so a fake
// kubectl can record a trace and compare it against the Python behaviour. The
// runner is injectable so unit tests never need a cluster.
package kube

import (
	"bytes"
	"errors"
	"fmt"
	"os/exec"
)

// Result is what one kubectl invocation produced.
type Result struct {
	Stdout string
	Stderr string
	Code   int
}

// Runner executes kubectl with the given arguments (without the binary name).
type Runner func(args []string) Result

// Exec returns a Runner that starts the kubectl binary named in the deploy
// script. A failed start (binary not found) is reported as exit status 1 with
// the start error on stderr, like the shell would.
func Exec(kubectl string) Runner {
	return func(args []string) Result {
		command := exec.Command(kubectl, args...)
		var stdout, stderr bytes.Buffer
		command.Stdout = &stdout
		command.Stderr = &stderr
		err := command.Run()
		result := Result{Stdout: stdout.String(), Stderr: stderr.String()}
		if err == nil {
			return result
		}
		var exit *exec.ExitError
		if errors.As(err, &exit) {
			result.Code = exit.ExitCode()
			return result
		}
		result.Code = 1
		if result.Stderr == "" {
			result.Stderr = fmt.Sprint(err)
		}
		return result
	}
}

// Fake returns a Runner that answers from a fixed table. It is the Go analogue
// of the Python tests' fake `_run`: every call is appended to the returned
// slice, and the handler decides the result.
func Fake(handler func(args []string) Result, calls *[][]string) Runner {
	return func(args []string) Result {
		copied := append([]string(nil), args...)
		*calls = append(*calls, copied)
		return handler(args)
	}
}
