"""Build-time version environment for the station binary.

The former Makefile passed PULSAR_VERSION and PULSAR_BUILD_TIME as environment
variables to cargo. Here the same values come from `--define` make variables
and are written to an env file that rustc reads (`rustc_env_files`), so the
Rust build stays a normal, correctly-keyed Bazel action.

The release passes the VERSION file's content and HEAD's commit date (the
former Makefile's reproducible BUILD_TIME); without them both default to "unknown".
"""

def _pulsar_version_env_impl(ctx):
    version = ctx.var.get("pulsar_version", "unknown")
    build_time = ctx.var.get("pulsar_build_time", "unknown")
    out = ctx.actions.declare_file(ctx.label.name + ".env")
    ctx.actions.write(
        output = out,
        content = "PULSAR_VERSION={}\nPULSAR_BUILD_TIME={}\n".format(
            version,
            build_time,
        ),
    )
    return [DefaultInfo(files = depset([out]))]

pulsar_version_env = rule(
    implementation = _pulsar_version_env_impl,
)
