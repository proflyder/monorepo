"""Build-time identity for the station BLE gateway.

`--version` reports the Cargo package version through the rules_rust `version`
attribute (without it `env!("CARGO_PKG_VERSION")` is `0.0.0`). The git
revision is optional and comes from `--define=ble_gateway_git_rev=<rev>`; it is
written to an env file that rustc reads (`rustc_env_files`), so the Rust build
stays a normal, correctly-keyed Bazel action. Without the define it is
`unknown`, matching the Pulsar build-time identity rule.

Usage:
    bazel build -c opt --platforms=//platform/bazel/zig:armhf_linux \
      --define=ble_gateway_git_rev=$(git rev-parse --short HEAD) \
      //projects/station/ble-gateway:ble-gateway
"""

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
