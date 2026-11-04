"""Strips the station binary and keeps a separate debug file.

The Makefile copied the unstripped binary to `pulsar.debug` and stripped the
deployed `pulsar`. This does the same (so `nm`/`addr2line` work on the debug
file) and additionally writes a `.gnu_debuglink` into the stripped binary, so
`addr2line -f -e pulsar.debug` finds it next to `pulsar` without extra flags.

objcopy is the zig wrapper of the selected station cc toolchain, so both
architectures are handled without a host binutils dependency.
"""

def _station_binary_impl(ctx):
    cc_toolchain = ctx.toolchains["@bazel_tools//tools/cpp:toolchain_type"].cc
    objcopy = cc_toolchain.objcopy_executable

    stripped = ctx.actions.declare_file("pulsar")
    debug = ctx.actions.declare_file("pulsar.debug")

    ctx.actions.run_shell(
        outputs = [stripped, debug],
        inputs = [ctx.file.binary],
        tools = cc_toolchain.all_files.to_list() +
                cc_toolchain._objcopy_files.to_list(),
        command = "\n".join([
            "set -eu",
            # objcopy.path is execroot-relative; the last call runs after cd, so
            # pin the absolute path while the action still starts in the execroot.
            "objcopy=\"$PWD/{objcopy}\"",
            "cp {src} {debug}",
            "\"$objcopy\" --strip-all {debug} {stripped}",
            "cd \"$(dirname {stripped})\"",
            # zig objcopy writes a new output file; it does not edit in place.
            "\"$objcopy\" --add-gnu-debuglink=\"$(basename {debug})\" \"$(basename {stripped})\" pulsar.linked",
            "mv pulsar.linked \"$(basename {stripped})\"",
        ]).format(
            src = ctx.file.binary.path,
            objcopy = objcopy,
            debug = debug.path,
            stripped = stripped.path,
        ),
    )

    return [DefaultInfo(files = depset([stripped, debug]))]

station_binary = rule(
    implementation = _station_binary_impl,
    attrs = {
        "binary": attr.label(
            allow_single_file = True,
            mandatory = True,
        ),
    },
    toolchains = ["@bazel_tools//tools/cpp:toolchain_type"],
)
