"""Builds one Rust binary for the static musl image target.

The image is built from a Go-free, fully static `x86_64-unknown-linux-musl`
binary. A transition is applied to the binary so the CI's usual
`--platforms=@rules_go//go/toolchain:linux_amd64` (meaning "the linux/amd64
image") does not accidentally package a glibc or host executable.
"""

def _musl_platform_transition_impl(settings, attr):
    _ignore = (settings, attr)
    return {
        "//command_line_option:compilation_mode": "opt",
        "//command_line_option:platforms": "//platform/bazel/zig:x86_64_musl_linux",
    }

_musl_platform_transition = transition(
    implementation = _musl_platform_transition_impl,
    inputs = [],
    outputs = [
        "//command_line_option:compilation_mode",
        "//command_line_option:platforms",
    ],
)

def _musl_binary_impl(ctx):
    target = ctx.attr.binary
    # A label with a configuration transition is a single Target in current
    # Bazel, but older/newer rule wrappers may expose a one-element sequence.
    if type(target) == "list":
        files = []
        for item in target:
            files.extend(item[DefaultInfo].files.to_list())
        return [DefaultInfo(files = depset(files))]
    return [target[DefaultInfo]]

musl_binary = rule(
    implementation = _musl_binary_impl,
    attrs = {
        "binary": attr.label(
            cfg = _musl_platform_transition,
            mandatory = True,
            executable = True,
        ),
    },
)
