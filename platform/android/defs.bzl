load("@rules_kotlin//kotlin:android.bzl", "kt_android_library", "kt_android_local_test")
load("@rules_kotlin//kotlin:jvm.bzl", "kt_jvm_library", "kt_jvm_test")
load("@rules_android//android:rules.bzl", "android_binary")
load(":lint.bzl", _android_lint = "android_lint")
load(":detekt.bzl", _home_detekt = "home_detekt")

METRO_COMPILER_PLUGIN = "//platform/android:metro_compiler_plugin"
SERIALIZATION_COMPILER_PLUGIN = "//platform/android:serialization_compiler_plugin"
JVM17_OPTIONS = "//platform/android:jvm17_options"

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

ROBOLECTRIC_ANDROID_ALL = "//platform/android:robolectric_android_all_sdk35"
ROBOLECTRIC_DEPS_PROPERTIES = "//platform/android:robolectric-deps.properties"
ROBOLECTRIC_JVM_FLAGS = [
    "-XX:ReservedCodeCacheSize=512m",
    "-Drobolectric.offline=true",
    "--add-opens=java.base/java.lang=ALL-UNNAMED",
    "--add-opens=java.base/java.io=ALL-UNNAMED",
    "--add-opens=java.base/java.net=ALL-UNNAMED",
    "--add-opens=java.base/java.security=ALL-UNNAMED",
    "--add-opens=java.base/java.text=ALL-UNNAMED",
    "--add-opens=java.base/java.util=ALL-UNNAMED",
    "--add-opens=java.base/java.util.concurrent=ALL-UNNAMED",
    "--add-opens=java.base/jdk.internal.util=ALL-UNNAMED",
]
LOCAL_TEST_SUITE_CLASS = "ModuleLocalTestSuite"

def _android_all_dir_impl(ctx):
    out = ctx.actions.declare_directory(ctx.label.name)
    ctx.actions.run_shell(
        inputs = [ctx.file.jar],
        outputs = [out],
        arguments = [out.path, ctx.file.jar.path, ctx.attr.jar_name],
        command = 'mkdir -p "$1" && cp "$2" "$1/$3"',
        mnemonic = "RobolectricAndroidAll",
        progress_message = "Staging Robolectric runtime %s" % ctx.attr.jar_name,
    )
    return [DefaultInfo(files = depset([out]), runfiles = ctx.runfiles(files = [out]))]

android_all_dir = rule(
    implementation = _android_all_dir_impl,
    attrs = {
        "jar": attr.label(allow_single_file = True, mandatory = True),
        "jar_name": attr.string(mandatory = True),
    },
)

def _local_test_suite_impl(ctx):
    out = ctx.actions.declare_file("%s/%s.kt" % (ctx.label.name, LOCAL_TEST_SUITE_CLASS))
    classes = "".join(["    %s::class,\n" % test_class for test_class in ctx.attr.test_classes])
    ctx.actions.write(
        out,
        "package %s\n\n@org.junit.runner.RunWith(org.junit.runners.Suite::class)\n@org.junit.runners.Suite.SuiteClasses(\n%s)\nclass %s\n" % (
            ctx.attr.package,
            classes,
            LOCAL_TEST_SUITE_CLASS,
        ),
    )
    return [DefaultInfo(files = depset([out]))]

local_test_suite = rule(
    implementation = _local_test_suite_impl,
    attrs = {
        "package": attr.string(mandatory = True),
        "test_classes": attr.string_list(mandatory = True),
    },
)

def home_android_local_test(name, srcs, test_classes, custom_package, data = [], jvm_flags = [], deps = [], **kwargs):
    local_test_suite(
        name = name + "_suite",
        package = custom_package,
        test_classes = test_classes,
    )
    kt_android_local_test(
        name = name,
        srcs = srcs + [":" + name + "_suite"],
        custom_package = custom_package,
        data = data + [ROBOLECTRIC_DEPS_PROPERTIES, ROBOLECTRIC_ANDROID_ALL],
        jvm_flags = ROBOLECTRIC_JVM_FLAGS + jvm_flags,
        robolectric_properties_file = ROBOLECTRIC_DEPS_PROPERTIES,
        test_class = custom_package + "." + LOCAL_TEST_SUITE_CLASS,
        deps = deps if "@maven//:junit_junit" in deps else deps + ["@maven//:junit_junit"],
        **kwargs
    )
