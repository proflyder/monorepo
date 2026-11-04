# Hermetic zig 0.14.1 SDK repository and the cc_toolchain_config macro.
#
# The SDK is downloaded once per host and the toolchain config is copied next
# to it, so `tool_path` entries stay relative to the repository package and no
# absolute host path leaks into the build. The wrappers pin the target triple,
# CPU and float ABI; zig also supplies the glibc version and static libc++.

_ZIG_VERSION = "0.14.1"

# Official ziglang.org release tarballs, pinned by sha256.
_SHA256 = {
    "linux-x86_64": "24aeeec8af16c381934a6cd7d95c807a8cb2cf7df9fa40d359aa884195c4716c",
    "linux-aarch64": "f7a654acc967864f7a050ddacfaa778c7504a0eca8d2b678839c21eea47c992b",
    "macos-aarch64": "39f3dc5e79c22088ce878edc821dedb4ca5a1cd9f5ef915e9b3cc3053e8faefa",
    "macos-x86_64": "b0f8bdfb9035783db58dd6c19d7dea89892acc3814421853e5752fe4573e5f43",
}

_WRAPPER = """#!/bin/bash
set -eu
DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
# Zig writes its caches outside the sandbox by default; keep them in TMPDIR,
# which Bazel makes writable for the action, and let a caller override it.
export ZIG_GLOBAL_CACHE_DIR="${ZIG_GLOBAL_CACHE_DIR:-${TMPDIR:-/tmp}/zig-global-cache}"
export ZIG_LOCAL_CACHE_DIR="${ZIG_LOCAL_CACHE_DIR:-${TMPDIR:-/tmp}/zig-local-cache}"
# The `cc` crate adds its own --target/-target (and rules_rust's cargo build
# script runner forwards it); this wrapper already pins the station triple.
argv=()
while (($#)); do
  case "$1" in
    --target=*) shift ;;
    --target|-target) shift 2 ;;
    -target=*) shift ;;
    -Wl,--fix-cortex-a53-843419) shift ;;
    *) argv+=("$1"); shift ;;
  esac
done
# rules_rust's cc toolchain wiring sets CC and CXX to the same gcc tool, so
# pick the C++ driver for C++ sources.
subcommand={subcommand}
if [ "$subcommand" = "cc" ]; then
  for arg in "${argv[@]}"; do
    case "$arg" in
      *.cc|*.cpp|*.cxx|*.C) subcommand=c++ ; break ;;
    esac
  done
fi
exec "$DIR/zig" "$subcommand" {args} "${argv[@]}"
"""

def _host_platform(ctx):
    os = ctx.os.name.lower()
    arch = ctx.os.arch.lower()
    if arch in ("amd64", "x86_64"):
        arch = "x86_64"
    elif arch in ("arm64", "aarch64"):
        arch = "aarch64"
    else:
        fail("unsupported zig exec host architecture: {}".format(arch))
    if os not in ("linux", "macos"):
        fail("unsupported zig exec host OS: {}".format(os))
    return os, arch

def _zig_sdk_repository_impl(ctx):
    os, arch = _host_platform(ctx)
    key = "{}-{}".format(os, arch)
    if key not in _SHA256:
        fail("no pinned zig {} archive for {}".format(_ZIG_VERSION, key))

    ctx.download_and_extract(
        url = [
            "https://ziglang.org/download/{v}/zig-{arch}-{os}-{v}.tar.xz".format(
                v = _ZIG_VERSION,
                arch = arch,
                os = os,
            ),
        ],
        sha256 = _SHA256[key],
        stripPrefix = "zig-{}-{}-{}".format(arch, os, _ZIG_VERSION),
    )

    # The toolchain config is maintained in the main repository and copied here
    # so the generated cc_toolchain rules can resolve their tools by package
    # relative paths.
    ctx.template("toolchain.bzl", Label("//platform/bazel/zig:toolchain.bzl"), {})
    _generate_wrappers(ctx)
    ctx.file(
        "BUILD.bazel",
        _BUILD_FILE.replace("{sdk_root}", str(ctx.path(""))),
    )

# One wrapper set per target: the triple, CPU and float ABI are baked into the
# wrapper instead of a per-action flag, matching the former Makefile/cargo-zigbuild
# invocation.
_WRAPPERS = [
    ("aarch64", "cc", "cc", "-target aarch64-linux-gnu.2.36"),
    ("aarch64", "cpp", "c++", "-target aarch64-linux-gnu.2.36 -isystem \"$DIR/lib/libcxx/include\" -isystem \"$DIR/lib/libcxxabi/include\""),
    ("aarch64", "ar", "ar", ""),
    ("aarch64", "ranlib", "ranlib", ""),
    ("aarch64", "ld", "ld.lld", "-target aarch64-linux-gnu.2.36"),
    ("aarch64", "objcopy", "objcopy", ""),
    ("armhf", "cc", "cc", "-target arm-linux-gnueabihf.2.23 -mcpu=cortex_a53"),
    ("armhf", "cpp", "c++", "-target arm-linux-gnueabihf.2.23 -mcpu=cortex_a53 -isystem \"$DIR/lib/libcxx/include\" -isystem \"$DIR/lib/libcxxabi/include\""),
    ("armhf", "ar", "ar", ""),
    ("armhf", "ranlib", "ranlib", ""),
    ("armhf", "ld", "ld.lld", "-target arm-linux-gnueabihf.2.23"),
    ("armhf", "objcopy", "objcopy", ""),
]

def _generate_wrappers(ctx):
    for prefix, name, subcommand, args in _WRAPPERS:
        ctx.file(
            "tool_{}_{}".format(prefix, name),
            _WRAPPER.replace("{subcommand}", subcommand).replace("{args}", args),
            executable = True,
        )
    # `nm`/`gcov` are never on the code path we build (Rust and static C/C++),
    # but Bazel expects a valid tool path for them.
    ctx.file(
        "noop",
        "#!/bin/sh\nexit 0\n",
        executable = True,
    )

_BUILD_FILE = """load(":toolchain.bzl", "zig_cc_toolchain")

package(default_visibility = ["//visibility:public"])

# Marks the cross-build configuration so the zig cc_toolchains are never
# selected for host/exec tools (which use the host compiler and host rustc).
constraint_setting(name = "station_setting")

constraint_value(
    name = "station",
    constraint_setting = ":station_setting",
)

filegroup(
    name = "zig_bin",
    srcs = ["zig"],
)

filegroup(
    name = "headers",
    srcs = glob(["lib/**"]),
)

filegroup(
    name = "noop_file",
    srcs = ["noop"],
)

zig_cc_toolchain(
    name = "aarch64",
    cpu = "aarch64",
    includes_arch = "aarch64",
    sdk_root = "{sdk_root}",
    target_compatible_with = [
        "@platforms//os:linux",
        "@platforms//cpu:aarch64",
        ":station",
    ],
)

zig_cc_toolchain(
    name = "armhf",
    cpu = "armv7",
    includes_arch = "arm",
    sdk_root = "{sdk_root}",
    target_compatible_with = [
        "@platforms//os:linux",
        "@platforms//cpu:armv7",
        ":station",
    ],
)
"""

zig_sdk_repository = repository_rule(
    implementation = _zig_sdk_repository_impl,
    local = False,
    attrs = {},
)
