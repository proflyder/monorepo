_CMDLINE_TOOLS = "cmdline-tools/23.0"
_PLATFORM = "platforms/android-36"

_MISSING_BUILD = """
package(default_visibility = ["//visibility:public"])

filegroup(
    name = "sdk",
    srcs = ["MISSING"],
)
"""

_BUILD = """
package(default_visibility = ["//visibility:public"])

filegroup(
    name = "sdk",
    srcs = glob(["sdk/**"]),
)
"""

def _sdk_home(rctx):
    for key in ["ANDROID_HOME", "ANDROID_SDK_ROOT"]:
        value = rctx.os.environ.get(key, "")
        if value:
            return value
    return ""

def _android_lint_sdk_impl(rctx):
    home = _sdk_home(rctx)
    tools = rctx.path(home + "/" + _CMDLINE_TOOLS) if home else None
    platform = rctx.path(home + "/" + _PLATFORM) if home else None
    if tools == None or not tools.exists or not platform.exists:
        rctx.file("MISSING", "Android Lint needs ANDROID_HOME with %s and %s installed\n" % (_CMDLINE_TOOLS, _PLATFORM))
        rctx.file("BUILD.bazel", _MISSING_BUILD)
        return
    rctx.symlink(tools, "sdk/" + _CMDLINE_TOOLS)
    rctx.symlink(platform, "sdk/" + _PLATFORM)
    rctx.file("BUILD.bazel", _BUILD)

android_lint_sdk = repository_rule(
    implementation = _android_lint_sdk_impl,
    environ = ["ANDROID_HOME", "ANDROID_SDK_ROOT"],
    local = True,
)
