load("@rules_kotlin//kotlin:android.bzl", "kt_android_library", "kt_android_local_test")
load("@rules_kotlin//kotlin:jvm.bzl", "kt_jvm_library", "kt_jvm_test")
load("@rules_android//android:rules.bzl", "android_binary")
load(":lint.bzl", _android_lint = "android_lint")
load(":detekt.bzl", _home_detekt = "home_detekt")

METRO_COMPILER_PLUGIN = "//platform/android:metro_compiler_plugin"
SERIALIZATION_COMPILER_PLUGIN = "//platform/android:serialization_compiler_plugin"
JVM17_OPTIONS = "//platform/android:jvm17_options"
ROBOLECTRIC_NO_ANDROID_ALL = "//platform/android:robolectric-none.properties"

def metro_jvm_library(name, srcs, deps = [], plugins = [], **kwargs):
    kt_jvm_library(
        name = name,
        srcs = srcs,
        deps = deps,
        kotlinc_opts = JVM17_OPTIONS,
        plugins = [METRO_COMPILER_PLUGIN, SERIALIZATION_COMPILER_PLUGIN] + plugins,
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
    if view_binding:
        layouts = [f for f in resource_files if "/res/layout/" in f and f.endswith(".xml")]
        if not custom_package:
            fail("view_binding requires custom_package")
        native.genrule(
            name = name + "_view_binding",
            srcs = layouts,
            outs = [name + "_view_binding/ViewBindings.kt"],
            cmd = "$(execpath //platform/android/tools/gen_view_binding) " +
                  "--package " + custom_package + " --output $@ $(SRCS)",
            tools = ["//platform/android/tools/gen_view_binding"],
        )
        srcs = srcs + [":" + name + "_view_binding"]
    kt_android_library(
        name = name,
        srcs = srcs,
        custom_package = custom_package,
        deps = deps,
        kotlinc_opts = JVM17_OPTIONS,
        manifest = manifest,
        plugins = [METRO_COMPILER_PLUGIN, SERIALIZATION_COMPILER_PLUGIN] + plugins,
        resource_files = resource_files,
        **kwargs
    )

def metro_jvm_test(name, srcs, deps = [], plugins = [], **kwargs):
    kt_jvm_test(
        name = name,
        srcs = srcs,
        deps = deps,
        kotlinc_opts = JVM17_OPTIONS,
        plugins = [METRO_COMPILER_PLUGIN, SERIALIZATION_COMPILER_PLUGIN] + plugins,
        **kwargs
    )

def home_jvm_library(name, srcs, deps = [], plugins = [], **kwargs):
    kt_jvm_library(
        name = name,
        srcs = srcs,
        deps = deps,
        kotlinc_opts = JVM17_OPTIONS,
        plugins = [SERIALIZATION_COMPILER_PLUGIN] + plugins,
        **kwargs
    )

def home_jvm_test(name, srcs, deps = [], plugins = [], **kwargs):
    kt_jvm_test(
        name = name,
        srcs = srcs,
        deps = deps,
        kotlinc_opts = JVM17_OPTIONS,
        plugins = [SERIALIZATION_COMPILER_PLUGIN] + plugins,
        **kwargs
    )

def home_android_lint(name, deps, app_module, config, **kwargs):
    _android_lint(
        name = name,
        deps = deps,
        app_module = app_module,
        config = config,
        **kwargs
    )

def home_detekt(name, deps, config, **kwargs):
    _home_detekt(
        name = name,
        deps = deps,
        config = config,
        **kwargs
    )

def home_google_services_resources(
        name,
        package_name,
        version_name = "$(home_version_name)",
        version_code = "$(home_version_code)"):
    native.genrule(
        name = name,
        srcs = ["//platform/android:google_services_json"],
        tools = ["//platform/android/tools/google_services"],
        outs = ["res/values/google_services.xml"],
        cmd = "\n".join([
            "set -e",
            "input=''",
            "for candidate in $(SRCS); do",
            "  case \"$$candidate\" in",
            "    */google-services.json) input=\"$$candidate\" ;;",
            "  esac",
            "done",
            "if [ -n \"$$input\" ]; then",
            "  $(execpath //platform/android/tools/google_services) \\",
            "    --input \"$$input\" \\",
            "    --package " + package_name + " \\",
            "    --version-name \"" + version_name + "\" \\",
            "    --version-code \"" + version_code + "\" \\",
            "    --output $@",
            "else",
            "  $(execpath //platform/android/tools/google_services) \\",
            "    --package " + package_name + " \\",
            "    --version-name \"" + version_name + "\" \\",
            "    --version-code \"" + version_code + "\" \\",
            "    --output $@",
            "fi",
        ]),
    )

def home_android_binary(
        name,
        manifest,
        deps,
        application_id = "dev.proflyder.home",
        version_code = "$(home_version_code)",
        version_name = "$(home_version_name)",
        **kwargs):
    if "debug_key" not in kwargs:
        kwargs["debug_key"] = "//platform/android:home_debug_keystore"
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

def home_android_local_test(name, robolectric_properties_file = ROBOLECTRIC_NO_ANDROID_ALL, **kwargs):
    kt_android_local_test(
        name = name,
        robolectric_properties_file = robolectric_properties_file,
        **kwargs
    )
