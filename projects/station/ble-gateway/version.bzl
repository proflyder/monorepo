def _ble_gateway_version_env_impl(ctx):
    revision = ctx.var.get("ble_gateway_git_rev", "unknown")
    out = ctx.actions.declare_file(ctx.label.name + ".env")
    ctx.actions.write(
        output = out,
        content = "BLE_GATEWAY_GIT_REV={}\n".format(revision),
    )
    return [DefaultInfo(files = depset([out]))]

ble_gateway_version_env = rule(
    implementation = _ble_gateway_version_env_impl,
)
