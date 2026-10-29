"""Shared Bazel macros for the Android client.

The root build contract is MODULE.bazel + Bazelisk. Android application code
lives under projects/mobile/android; this package only carries toolchain and
macro wiring that can be reused by a future Android project.

Metro's compiler plugin is added to every Kotlin compilation by these macros.
A module can still pass extra plugins with `plugins = [...]`.
"""

load("@rules_kotlin//kotlin:android.bzl", "kt_android_library")
load("@rules_kotlin//kotlin:jvm.bzl", "kt_jvm_library", "kt_jvm_test")
load("@rules_android//android:rules.bzl", "android_binary")

METRO_COMPILER_PLUGIN = "//platform/android:metro_compiler_plugin"
JVM17_OPTIONS = "//platform/android:jvm17_options"

def metro_jvm_library(name, srcs, deps = [], plugins = [], **kwargs):
    """Kotlin/JVM library with the Metro compiler plugin enabled."""
    kt_jvm_library(
        name = name,
        srcs = srcs,
        deps = deps,
        kotlinc_opts = JVM17_OPTIONS,
        plugins = [METRO_COMPILER_PLUGIN] + plugins,
        **kwargs
    )

def metro_android_library(
        name,
        srcs,
        deps = [],
        resource_files = [],
        manifest = None,
        custom_package = None,
        plugins = [],
        view_binding = False,
        **kwargs):
    """Kotlin Android library with real Android resources and Metro DI.

    With `view_binding = True` a `<Layout>Binding` class is generated for every
    `res/layout/*.xml` in `resource_files` (package `<custom_package>.databinding`).
    The module must depend on //projects/mobile/android/core/ui:ui, which owns
    the `ViewBinding` interface the generated classes implement.
    """
    if view_binding:
        layouts = [f for f in resource_files if "/res/layout/" in f and f.endswith(".xml")]
        if not custom_package:
            fail("view_binding requires custom_package")
        native.genrule(
            name = name + "_view_binding",
            srcs = layouts,
            outs = [name + "_view_binding/ViewBindings.kt"],
            cmd = "python3 $(location //platform/android:gen_view_binding.py) " +
                  "--package " + custom_package + " --output $@ $(SRCS)",
            tools = ["//platform/android:gen_view_binding.py"],
        )
        srcs = srcs + [":" + name + "_view_binding"]
    kt_android_library(
        name = name,
        srcs = srcs,
        custom_package = custom_package,
        deps = deps,
        kotlinc_opts = JVM17_OPTIONS,
        manifest = manifest,
        plugins = [METRO_COMPILER_PLUGIN] + plugins,
        resource_files = resource_files,
        **kwargs
    )

def metro_jvm_test(name, srcs, deps = [], plugins = [], **kwargs):
    """JUnit4 Kotlin test with the Metro compiler plugin enabled."""
    kt_jvm_test(
        name = name,
        srcs = srcs,
        deps = deps,
        kotlinc_opts = JVM17_OPTIONS,
        plugins = [METRO_COMPILER_PLUGIN] + plugins,
        **kwargs
    )

def home_android_binary(
        name,
        manifest,
        deps,
        application_id = "dev.proflyder.home",
        version_code = "1",
        version_name = "0.1.0",
        **kwargs):
    """debug-signed application APK for the home client."""
    android_binary(
        name = name,
        custom_package = application_id,
        manifest = manifest,
        manifest_values = {
            "applicationId": application_id,
            "versionCode": version_code,
            "versionName": version_name,
        },
        deps = deps,
        **kwargs
    )
