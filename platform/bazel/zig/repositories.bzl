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
{windows_args}    *) argv+=("$1"); shift ;;
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
# rustc passes `-O1` when linking an optimized binary. Zig's link driver
# ignores it but prints "ignoring deprecated linker optimization setting",
# which rustc 1.97+ forwards through the linker_messages lint. The flag only
# affects linking here, so drop -O levels from link invocations; compile -O
# (from cc-rs CFLAGS/CXXFLAGS) must stay.
has_compile=0
for arg in "${argv[@]}"; do
  case "$arg" in
    -c|-E|-S) has_compile=1 ;;
  esac
done
if [ "$has_compile" = 0 ]; then
  filtered=()
  for arg in "${argv[@]}"; do
    case "$arg" in
      -O0|-O1|-O2|-O3|-Os|-Oz) ;;
      -Wl,-O0|-Wl,-O1|-Wl,-O2|-Wl,-O3|-Wl,-Os|-Wl,-Oz) ;;
      *) filtered+=("$arg") ;;
    esac
  done
  argv=(${filtered[@]+"${filtered[@]}"})
fi
# `zig cc --version` parses the pinned `-target <triple>.<glibc>` as a native
# Zig target and rejects the glibc-version suffix, which cc-rs reports as a
# build-script warning. Compiler-identity probes do not need a target.
probe=0
for arg in "${argv[@]}"; do
  case "$arg" in
    --version|-dumpversion|-dumpmachine|-\\?) probe=1 ;;
  esac
done
target_args=({args})
if [ "$probe" = 1 ]; then target_args=(); fi
{musl_link}exec "$DIR/zig" "$subcommand" ${target_args[@]+"${target_args[@]}"} "${argv[@]}"
"""

# rules_rust drives the linker directly on windows-gnu targets and therefore
# needs `dlltool` for the raw-dylib imports used by the official windows-rs
# crates. Zig ships llvm-dlltool as `zig dlltool`; the wrapper is generated next
# to the linker so `-C dlltool=<linker dir>/dlltool` resolves inside the
# sandbox, with no host toolchain.
_DLLTOOL_WRAPPER = """#!/bin/bash
set -eu
DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
exec "$DIR/zig" dlltool "$@"
"""

# rustc's windows-gnu link line is written for a MinGW gcc driver. Zig's `cc`
# rejects a positional `.def` path passed through `-Wl,`, cannot resolve
# `-lmsvcrt` (it links UCRT import libraries itself), and does not accept
# rustc's `-nodefaultlibs`/mingw runtime library set. These translations only
# apply to the windows wrappers; the station wrappers keep the empty value.
_WINDOWS_ARGS = """    -Wl,*.def) argv+=("${1#-Wl,}"); shift ;;
    -l:libpthread.a) argv+=("-lwinpthread"); shift ;;
    # zig supplies UCRT/mingw-w64 defaults itself; rustc's gcc-driver flags
    # (and the -O1/auto-image-base options zig ignores with a warning) are
    # dropped.
    -nodefaultlibs|-lmsvcrt|-Wl,-O1|-Wl,--disable-auto-image-base) shift ;;
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
    # Bazel reports the host OS as `ctx.os.name`, which is not the zig archive
    # spelling: macOS is "mac os x" (older Bazel also "macos"/"darwin"), Linux
    # is "linux". Normalise the prefix, keep the explicit refusal for anything
    # else so an unsupported host can never silently pick a wrong archive.
    if os.startswith("mac") or os.startswith("darwin"):
        os = "macos"
    elif os.startswith("linux"):
        os = "linux"
    else:
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
    # Windows Credential Provider (solutions/windows-logon-provider). Zig's
    # x86_64-windows-gnu target uses mingw-w64/UCRT; the resulting PE imports
    # only system DLLs.
    # Static musl service images (wakegrant). Fully static libc, no glibc.
    ("x86_64_musl", "cc", "cc", "-target x86_64-linux-musl"),
    ("x86_64_musl", "cpp", "c++", "-target x86_64-linux-musl -isystem \"$DIR/lib/libcxx/include\" -isystem \"$DIR/lib/libcxxabi/include\""),
    ("x86_64_musl", "ar", "ar", ""),
    ("x86_64_musl", "ranlib", "ranlib", ""),
    ("x86_64_musl", "ld", "ld.lld", "-target x86_64-linux-musl"),
    ("x86_64_musl", "objcopy", "objcopy", ""),
    ("windows_x86_64", "cc", "cc", "-target x86_64-windows-gnu"),
    ("windows_x86_64", "cpp", "c++", "-target x86_64-windows-gnu -isystem \"$DIR/lib/libcxx/include\" -isystem \"$DIR/lib/libcxxabi/include\""),
    ("windows_x86_64", "ar", "ar", ""),
    ("windows_x86_64", "ranlib", "ranlib", ""),
    ("windows_x86_64", "ld", "ld.lld", "-target x86_64-windows-gnu"),
    ("windows_x86_64", "objcopy", "objcopy", ""),
]

def _generate_wrappers(ctx):
    for prefix, name, subcommand, args in _WRAPPERS:
        windows_args = _WINDOWS_ARGS if prefix == "windows_x86_64" else ""
        # Rust's musl link line already carries the self-contained crt and
        # libc, but zig's cc driver adds its own crt1.o, which duplicates
        # `_start`. `-nostdlib` is only valid on a link, not on a compile
        # (it would also drop zig's libc include paths).
        musl_link = ""
        if prefix == "x86_64_musl" and name in ("cc", "cpp"):
            musl_link = """has_compile=0
for arg in "${argv[@]}"; do
  case "$arg" in
    -c|-E|-S) has_compile=1 ;;
  esac
done
if [ "$has_compile" = 0 ]; then
  argv+=("-nostdlib")
fi
"""
        ctx.file(
            "tool_{}_{}".format(prefix, name),
            _WRAPPER.replace("{subcommand}", subcommand)
            .replace("{args}", args)
            .replace("{windows_args}", windows_args)
            .replace("{musl_link}", musl_link),
            executable = True,
        )
    # `nm`/`gcov` are never on the code path we build (Rust and static C/C++),
    # but Bazel expects a valid tool path for them.
    ctx.file(
        "noop",
        "#!/bin/sh\nexit 0\n",
        executable = True,
    )
    # rules_rust derives `-C dlltool=<linker dir>/dlltool` for windows targets.
    ctx.file(
        "dlltool",
        _DLLTOOL_WRAPPER,
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

# Static musl services: only //platform/bazel/zig:x86_64_musl_linux carries
# this value, so no host or glibc build can select the musl cc_toolchain.
constraint_setting(name = "musl_setting")

constraint_value(
    name = "musl",
    constraint_setting = ":musl_setting",
)

# Same idea for the Windows credential provider target: only
# //platform/bazel/windows:x86_64_windows carries this value, so host builds and
# the station triples never select the windows cc_toolchain.
constraint_setting(name = "windows_setting")

constraint_value(
    name = "windows",
    constraint_setting = ":windows_setting",
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

filegroup(
    name = "dlltool_file",
    srcs = ["dlltool"],
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

zig_cc_toolchain(
    name = "x86_64_musl",
    cpu = "x86_64_musl",
    includes_arch = "x86_64",
    libc = "musl",
    sdk_root = "{sdk_root}",
    target_compatible_with = [
        "@platforms//os:linux",
        "@platforms//cpu:x86_64",
        ":musl",
    ],
)

zig_cc_toolchain(
    name = "windows_x86_64",
    cpu = "x86_64_windows",
    includes_arch = "x86_64",
    sdk_root = "{sdk_root}",
    target_compatible_with = [
        "@platforms//os:windows",
        "@platforms//cpu:x86_64",
        ":windows",
    ],
    target_os = "windows",
)
"""

zig_sdk_repository = repository_rule(
    implementation = _zig_sdk_repository_impl,
    local = False,
    attrs = {},
)
