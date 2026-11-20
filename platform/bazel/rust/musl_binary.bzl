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
