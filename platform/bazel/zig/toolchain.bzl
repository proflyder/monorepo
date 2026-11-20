load("@bazel_tools//tools/cpp:cc_toolchain_config_lib.bzl", "feature", "flag_group", "flag_set")
load("@rules_cc//cc:defs.bzl", "CcToolchainConfigInfo", "cc_common", "cc_toolchain")

def _zig_cc_toolchain_config_impl(ctx):
    base = ctx.attr.sdk_root

    if ctx.attr.libc == "musl":
        include_dirs = [
            base + "/lib/libc/include/generic-musl",
            base + "/lib/libc/include/" + ctx.attr.includes_arch + "-linux-musl",
            base + "/lib/libc/include/" + ctx.attr.includes_arch + "-linux-any",
            base + "/lib/libc/include/any-linux-any",
            base + "/lib/libcxx/include",
            base + "/lib/libcxxabi/include",
            base + "/lib/include",
        ]
        target_libc = "musl"
    elif ctx.attr.target_os == "windows":
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
        "libc": attr.string(default = "glibc"),
        "sdk_root": attr.string(mandatory = True),
        "target": attr.string(mandatory = True),
        "target_os": attr.string(default = "linux"),
    },
    provides = [CcToolchainConfigInfo],
)

def zig_cc_toolchain(name, cpu, includes_arch, sdk_root, target_compatible_with, libc = "glibc", target_os = "linux"):
    triple = {
        "aarch64": "aarch64-linux-gnu.2.36",
        "armv7": "arm-linux-gnueabihf.2.23",
        "x86_64_musl": "x86_64-linux-musl",
        "x86_64_windows": "x86_64-windows-gnu",
    }[cpu]

    _zig_cc_toolchain_config(
        name = name + "_config",
        cpu = cpu,
        includes_arch = includes_arch,
        libc = libc,
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
