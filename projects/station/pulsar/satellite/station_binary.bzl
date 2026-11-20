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
            "objcopy=\"$PWD/{objcopy}\"",
            "cp {src} {debug}",
            "\"$objcopy\" --strip-all {debug} {stripped}",
            "cd \"$(dirname {stripped})\"",
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
