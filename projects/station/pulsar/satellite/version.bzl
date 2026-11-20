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
