load("@rules_oci//oci:defs.bzl", "oci_image", "oci_load", "oci_push")
load("@rules_pkg//pkg:tar.bzl", "pkg_tar")
load("//platform/bazel/rust:musl_binary.bzl", "musl_binary")

BASE = "@distroless_static_linux_amd64"


def service_image(
        name,
        binary,
        repo,
        user = "65534:65534",
        extra_tars = [],
        env = {},
        entrypoint = None):
    layer = name + "_layer"
    image = name + "_image"
    pkg_tar(
        name = layer,
        srcs = [binary],
        package_dir = "/usr/local/bin",
        mode = "0755",
        visibility = ["//visibility:private"],
    )
    kwargs = {}
    if env:
        kwargs["env"] = env
    if entrypoint:
        kwargs["entrypoint"] = entrypoint
    else:
        kwargs["entrypoint"] = ["/usr/local/bin/" + name]
    oci_image(
        name = image,
        base = BASE,
        tars = [":" + layer] + list(extra_tars),
        user = user,
        visibility = ["//visibility:public"],
        **kwargs
    )
    oci_load(
        name = name + "_load",
        image = ":" + image,
        repo_tags = ["%s:local" % repo],
        visibility = ["//visibility:public"],
    )
    oci_push(
        name = name + "_push",
        image = ":" + image,
        repository = repo,
        visibility = ["//visibility:public"],
    )


def rust_service_image(
        name,
        crate_binary,
        repo,
        user = "65534:65534",
        extra_tars = [],
        env = {},
        entrypoint = None):
    binary = name + "_binary"
    musl_binary(
        name = binary,
        binary = crate_binary,
    )
    service_image(
        name = name,
        binary = ":" + binary,
        repo = repo,
        user = user,
        extra_tars = extra_tars,
        env = env,
        entrypoint = entrypoint,
    )
