"""Pins Robolectric's android-all runtime jar into a directory for offline tests."""

def _android_all_dir_impl(ctx):
    out = ctx.actions.declare_directory(ctx.label.name)
    jar = ctx.file.jar
    ctx.actions.run_shell(
        inputs = [jar],
        outputs = [out],
        arguments = [out.path, jar.path, ctx.attr.jar_name],
        command = 'mkdir -p "$1" && cp "$2" "$1/$3"',
        mnemonic = "RobolectricAndroidAll",
        progress_message = "Staging Robolectric runtime %s" % ctx.attr.jar_name,
    )
    return [
        DefaultInfo(
            files = depset([out]),
            runfiles = ctx.runfiles(files = [out]),
        ),
    ]

android_all_dir = rule(
    implementation = _android_all_dir_impl,
    attrs = {
        "jar": attr.label(allow_single_file = True, mandatory = True),
        "jar_name": attr.string(mandatory = True),
    },
    doc = "Copies one Maven jar into a directory with Robolectric's expected file name.",
)
