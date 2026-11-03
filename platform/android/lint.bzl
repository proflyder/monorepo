"""Bazel driven Android Lint for the Home Android client.

rules_android 0.7.x has no lint rule, so this uses the standalone Android Lint
CLI from the SDK `cmdline-tools` package. An aspect walks the Bazel dependency
graph once and records every module's sources, resources, manifest, dependency
edges and compile classpath; `android_lint` turns those records into lint's
`project.xml` model and runs lint over the whole project. Cross-module resource
usage is therefore analysed as one build instead of unrelated modules.
"""

load("@rules_android//providers:providers.bzl", "StarlarkAndroidResourcesInfo")
load("@rules_java//java:defs.bzl", "JavaInfo")

LintModuleInfo = provider(
    doc = "One Android or JVM module for the lint project descriptor.",
    fields = {
        "modules": "list of module structs (this target and its transitive deps)",
    },
)

def _module_files(targets):
    files = []
    for target in targets:
        files.extend(target[DefaultInfo].files.to_list())
    return files

def _lint_aspect_impl(target, ctx):
    modules = []

    srcs = _module_files(getattr(ctx.rule.attr, "srcs", []))
    resources = _module_files(getattr(ctx.rule.attr, "resource_files", []))
    manifest = None
    manifest_target = getattr(ctx.rule.attr, "manifest", None)
    if manifest_target != None:
        manifest_files = manifest_target[DefaultInfo].files.to_list()
        if manifest_files:
            manifest = manifest_files[0]

    deps = getattr(ctx.rule.attr, "deps", [])
    dep_labels = [str(dep.label) for dep in deps]
    for dep in deps:
        if LintModuleInfo in dep:
            modules.extend(dep[LintModuleInfo].modules)

    # Only real modules (sources or an Android manifest) get a descriptor;
    # Maven aar_import/jar targets only contribute classpath entries.
    if srcs or manifest != None:
        jars = depset()
        if JavaInfo in target:
            jars = target[JavaInfo].transitive_compile_time_jars
        android = manifest != None or StarlarkAndroidResourcesInfo in target
        modules.append(struct(
            label = str(target.label),
            name = str(target.label),
            package = getattr(ctx.rule.attr, "custom_package", "") or "",
            android = android,
            srcs = srcs,
            resources = resources,
            manifest = manifest,
            deps = dep_labels,
            jars = jars,
        ))

    return [LintModuleInfo(modules = modules)]

_lint_aspect = aspect(
    attr_aspects = ["deps"],
    implementation = _lint_aspect_impl,
)

def _android_lint_impl(ctx):
    modules = {}
    for dep in ctx.attr.deps:
        if LintModuleInfo not in dep:
            continue
        for module in dep[LintModuleInfo].modules:
            modules[module.label] = module

    records = []
    files = []
    for label in sorted(modules):
        module = modules[label]
        files.extend(module.srcs)
        files.extend(module.resources)
        if module.manifest != None:
            files.append(module.manifest)
        files.extend(module.jars.to_list())
        records.append(json.encode({
            "label": module.label,
            "name": module.name,
            "package": module.package,
            "android": module.android,
            "app": module.label == ctx.attr.app_module,
            "srcs": [f.short_path for f in module.srcs],
            "generated_srcs": [f.short_path for f in module.srcs if not f.is_source],
            "resources": [f.short_path for f in module.resources],
            "manifest": module.manifest.short_path if module.manifest != None else None,
            "deps": module.deps,
            "jars": [f.short_path for f in module.jars.to_list()],
        }))

    records_file = ctx.actions.declare_file(ctx.label.name + ".modules.jsonl")
    ctx.actions.write(records_file, "\n".join(records) + "\n")

    runner = ctx.actions.declare_file(ctx.label.name + ".run")
    runner_content = "\n".join([
        "#!/usr/bin/env bash",
        "set -euo pipefail",
        "cd \"$TEST_SRCDIR/$TEST_WORKSPACE\"",
        # rules_python's bootstrap stub selects its hermetic runtime, but the
        # stub itself needs a working `python3` on PATH. A broken version-manager
        # shim must not hide a usable system interpreter.
        "if ! python3 -c '' >/dev/null 2>&1; then",
        "  for prefix in /opt/homebrew/bin /usr/local/bin /usr/bin /bin; do",
        "    if \"$prefix/python3\" -c '' >/dev/null 2>&1; then",
        "      PATH=\"$prefix:$PATH\"",
        "      export PATH",
        "      break",
        "    fi",
        "  done",
        "fi",
        "exec \"$TEST_SRCDIR/$TEST_WORKSPACE/%s\" \"$TEST_SRCDIR/$TEST_WORKSPACE/%s\" \"$TEST_SRCDIR/$TEST_WORKSPACE/%s\"" % (
            ctx.executable._runner.short_path,
            records_file.short_path,
            ctx.file.config.short_path,
        ),
        "",
    ])
    ctx.actions.write(runner, runner_content, is_executable = True)

    runfiles = ctx.runfiles(files = files + [records_file, runner, ctx.file.config])
    runfiles = runfiles.merge(ctx.attr._runner[DefaultInfo].default_runfiles)
    return [
        DefaultInfo(executable = runner, runfiles = runfiles),
        RunEnvironmentInfo(
            environment = {
                "LINT_MODULES": records_file.short_path,
            },
            inherited_environment = [
                "ANDROID_HOME",
                "ANDROID_SDK_ROOT",
                "ANDROID_LINT",
                "JAVA_HOME",
                "PATH",
            ],
        ),
    ]

_android_lint_test = rule(
    implementation = _android_lint_impl,
    attrs = {
        "deps": attr.label_list(aspects = [_lint_aspect]),
        "app_module": attr.string(
            doc = "Label of the application module (library=false); all others are libraries.",
        ),
        "config": attr.label(
            doc = "lint.xml configuration file.",
            allow_single_file = True,
        ),
        "_runner": attr.label(
            default = "//platform/android:lint_runner",
            executable = True,
            cfg = "exec",
        ),
    },
    executable = True,
    test = True,
)

def android_lint(name, deps, app_module, config, **kwargs):
    """Lint every module reachable from `deps` as one Android project."""
    _android_lint_test(
        name = name,
        deps = deps,
        app_module = app_module,
        config = config,
        **kwargs
    )
