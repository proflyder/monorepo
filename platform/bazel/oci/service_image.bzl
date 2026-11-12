"""Shared one-binary distroless service images and the Rust musl variant."""

load("@rules_oci//oci:defs.bzl", "oci_image", "oci_load", "oci_push")
load("@rules_pkg//pkg:tar.bzl", "pkg_tar")
load("//platform/bazel/rust:musl_binary.bzl", "musl_binary")

# Digest-pinned distroless base (see MODULE.bazel `oci.pull`).
BASE = "@distroless_static_linux_amd64"


def service_image(
        name,
        binary,
        repo,
        user = "65534:65534",
        extra_tars = [],
        env = {},
        entrypoint = None):
    """One static binary on distroless, plus a loadable tar and push target.

    Args:
      name: target prefix (`<name>_image`, `<name>_load`, `<name>_push`).
      binary: the executable to package at `/usr/local/bin`.
      repo: GHCR repository for the push target.
      user: runtime user recorded in the image.
      extra_tars: optional additional layers (e.g. a read-only data overlay).
      env: optional environment variables baked into the image.
      entrypoint: optional entrypoint override; defaults to the package name.

    The defaults keep every existing image byte-identical: with no `extra_tars`,
    no `env` and no `entrypoint` the produced manifest is unchanged.
    """
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
        # rules_oci writes a fixed creation time, so the digest is reproducible.
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
    """A Rust service image: the crate binary rebuilt for static musl, then
    packaged as one distroless layer (plus optional extra layers)."""
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
