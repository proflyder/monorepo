"""Hermetic zig cc_toolchain config.

This file is copied into the zig SDK repository by `zig_sdk_repository`, so
every tool path is relative to that repository's package and the toolchain has
no host-absolute paths. It is not loaded from the main repository.

Targets: aarch64-linux-gnu (glibc 2.36), arm-linux-gnueabihf (glibc 2.23,
cortex-a53 / VFPv4) and x86_64-windows-gnu (mingw-w64/UCRT), matching the
former satellite/Makefile and cargo-zigbuild, and the Windows credential
provider.
"""

load("@bazel_tools//tools/cpp:cc_toolchain_config_lib.bzl", "feature", "flag_group", "flag_set")
load("@rules_cc//cc:defs.bzl", "CcToolchainConfigInfo", "cc_common", "cc_toolchain")

def _zig_cc_toolchain_config_impl(ctx):
    # `%crosstool_top%` resolves to the package of this cc_toolchain (the zig
    # Absolute path of the fetched SDK. Bazel's include checker compares the
    # compiler's absolute header paths against these, so they cannot be
    # execroot-relative.
    base = ctx.attr.sdk_root

    if ctx.attr.target_os == "windows":
        # mingw-w64/UCRT headers plus zig's libc++ for C++ sources. The
        # Windows provider itself is pure Rust; these paths keep a future C/C++
        # dependency compilable with the same toolchain.
        include_dirs = [
            base + "/lib/libc/include/any-windows-any",
            base + "/lib/libc/mingw/include",
            base + "/lib/libc/mingw/crt",
            base + "/lib/libcxx/include",
            base + "/lib/libcxxabi/include",
            base + "/lib/include",
        ]
        target_libc = "ucrt"
    else:
        include_dirs = [
            base + "/lib/libc/include/generic-glibc",
            base + "/lib/libc/include/" + ctx.attr.includes_arch + "-linux-gnu",
            base + "/lib/libc/include/" + ctx.attr.includes_arch + "-linux-gnueabihf",
            base + "/lib/libc/include/" + ctx.attr.includes_arch + "-linux-any",
            base + "/lib/libc/include/any-linux-any",
            base + "/lib/libcxx/include",
            base + "/lib/libcxxabi/include",
            base + "/lib/include",
            base + "/lib/unwind/include",
        ]
        target_libc = "glibc"

    prefix = ctx.attr.prefix
    # rules_rust derives `-C dlltool=<linker dir>/dlltool` for windows targets
    # and rustc runs dlltool from its temporary codegen directory, so a
    # relative path is not resolvable there. The windows tool paths are
    # therefore absolute (the SDK lives in the external repository, which Bazel
    # maps into the sandbox as a declared input). Station toolchains keep the
    # relative paths they always used.
    tool_prefix = (base + "/" + prefix) if ctx.attr.target_os == "windows" else prefix
    tool_paths = [
        struct(name = "gcc", path = tool_prefix + "_cc"),
        struct(name = "cpp", path = tool_prefix + "_cpp"),
        struct(name = "ar", path = tool_prefix + "_ar"),
        struct(name = "ld", path = tool_prefix + "_cc"),
        struct(name = "nm", path = "noop"),
        struct(name = "objcopy", path = tool_prefix + "_objcopy"),
        struct(name = "objdump", path = "noop"),
        struct(name = "strip", path = tool_prefix + "_objcopy"),
        struct(name = "gcov", path = "noop"),
        struct(name = "llvm-cov", path = "noop"),
        struct(name = "dwp", path = "noop"),
    ]

    # The CMake Release build of the sendspin libraries is -O3 -DNDEBUG. The
    # bare toolchain config otherwise passes no -O flag and zig compiles all
    # C/C++ at -O0 (roughly twice the deployed binary size).
    def compile_flags_feature(name, flags):
        return feature(
            name = name,
            flag_sets = [
                flag_set(
                    actions = ["c-compile", "c++-compile"],
                    flag_groups = [flag_group(flags = flags)],
                ),
            ],
        )

    return cc_common.create_cc_toolchain_config_info(
        ctx = ctx,
        toolchain_identifier = ctx.attr.name,
        host_system_name = "local",
        target_system_name = ctx.attr.target,
        target_cpu = ctx.attr.cpu,
        target_libc = target_libc,
        compiler = "zig",
        abi_version = "local",
        abi_libc_version = "local",
        tool_paths = tool_paths,
        cxx_builtin_include_directories = include_dirs,
        features = [
            compile_flags_feature("opt", ["-O3", "-DNDEBUG"]),
            compile_flags_feature("dbg", ["-O0", "-g"]),
            compile_flags_feature("fastbuild", ["-O0"]),
        ],
    )

_zig_cc_toolchain_config = rule(
    implementation = _zig_cc_toolchain_config_impl,
    attrs = {
        "cpu": attr.string(mandatory = True),
        "includes_arch": attr.string(mandatory = True),
        "prefix": attr.string(mandatory = True),
        "sdk_root": attr.string(mandatory = True),
        "target": attr.string(mandatory = True),
        "target_os": attr.string(default = "linux"),
    },
    provides = [CcToolchainConfigInfo],
)

def zig_cc_toolchain(name, cpu, includes_arch, sdk_root, target_compatible_with, target_os = "linux"):
    """Declares the config, cc_toolchain and Bazel toolchain for one target."""
    triple = {
        "aarch64": "aarch64-linux-gnu.2.36",
        "armv7": "arm-linux-gnueabihf.2.23",
        "x86_64_windows": "x86_64-windows-gnu",
    }[cpu]

    _zig_cc_toolchain_config(
        name = name + "_config",
        cpu = cpu,
        includes_arch = includes_arch,
        prefix = "tool_" + name,
        sdk_root = sdk_root,
        target = triple,
        target_os = target_os,
    )

    native.filegroup(
        name = name + "_files",
        srcs = [
            ":headers",
            ":noop_file",
            ":zig_bin",
            # rules_rust derives the dlltool path from the linker directory for
            # every windows compile, so the wrapper must be an action input.
            ":dlltool_file",
        ] + native.glob(["tool_" + name + "_*"]),
    )

    cc_toolchain(
        name = name + "_cc",
        all_files = ":" + name + "_files",
        ar_files = ":" + name + "_files",
        as_files = ":" + name + "_files",
        compiler_files = ":" + name + "_files",
        dwp_files = ":" + name + "_files",
        # rules_rust passes `-C dlltool=<linker dir>/dlltool` for windows
        # targets and derives it from this toolchain's linker path, so the
        # wrapper must travel with the linker files.
        linker_files = ":" + name + "_files",
        objcopy_files = ":" + name + "_files",
        strip_files = ":" + name + "_files",
        supports_param_files = 0,
        toolchain_config = ":" + name + "_config",
        toolchain_identifier = name,
    )

    native.toolchain(
        name = name + "_toolchain",
        exec_compatible_with = [],
        target_compatible_with = target_compatible_with,
        toolchain = ":" + name + "_cc",
        toolchain_type = "@bazel_tools//tools/cpp:toolchain_type",
    )
