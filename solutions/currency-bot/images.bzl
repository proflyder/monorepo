"""Macro for one-binary distroless service images."""

load("@rules_oci//oci:defs.bzl", "oci_image", "oci_load", "oci_push")
load("@rules_pkg//pkg:tar.bzl", "pkg_tar")

# Digest-pinned distroless base (see MODULE.bazel `oci.pull`).
BASE = "@distroless_static_linux_amd64"


def service_image(name, binary, repo, user = "65534:65534"):
    """One static Go binary on distroless, plus a loadable tar and push target."""
    layer = name + "_layer"
    image = name + "_image"
    pkg_tar(
        name = layer,
        srcs = [binary],
        package_dir = "/usr/local/bin",
        mode = "0755",
        visibility = ["//visibility:private"],
    )
    oci_image(
        name = image,
        base = BASE,
        tars = [":" + layer],
        # rules_oci writes a fixed creation time, so the digest is reproducible.
        entrypoint = ["/usr/local/bin/" + name],
        user = user,
        visibility = ["//visibility:public"],
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
