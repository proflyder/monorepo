load("@rules_android//providers:providers.bzl", "StarlarkAndroidResourcesInfo")
load("@rules_java//java:defs.bzl", "JavaInfo")

LintModuleInfo = provider(
    doc = "Android and JVM modules reachable through deps, for per-module Android Lint analysis.",
    fields = {
        "modules": "dict of module label to module struct (this target and its transitive deps)",
        "labels": "depset of module labels reachable from this target, itself included",
        "coordinates": "depset of (jar path, maven coordinate) pairs for imported Maven artifacts",
    },
)

def _module_files(targets):
    return [f for target in targets for f in target[DefaultInfo].files.to_list()]

def _maven_coordinates(target, ctx):
    if JavaInfo not in target:
        return []
    for tag in getattr(ctx.rule.attr, "tags", []):
        if tag.startswith("maven_coordinates="):
            coordinate = tag[len("maven_coordinates="):].split("@")[0]
            jars = []
            for output in target[JavaInfo].java_outputs:
                jars.extend([jar for jar in [output.compile_jar, output.class_jar] if jar != None])
            return [(jar.path, coordinate) for jar in jars]
    return []

def _lint_aspect_impl(target, ctx):
    deps = getattr(ctx.rule.attr, "deps", [])
    dep_infos = [dep[LintModuleInfo] for dep in deps if LintModuleInfo in dep]
    dep_labels = depset(transitive = [info.labels for info in dep_infos])
    dep_modules = {}
    for info in dep_infos:
        dep_modules.update(info.modules)
    linked = deps + getattr(ctx.rule.attr, "exports", [])
    coordinates = depset(
        _maven_coordinates(target, ctx),
        transitive = [dep[LintModuleInfo].coordinates for dep in linked if LintModuleInfo in dep],
    )

    srcs = _module_files(getattr(ctx.rule.attr, "srcs", []))
    manifest = None
    manifest_target = getattr(ctx.rule.attr, "manifest", None)
    if manifest_target != None:
        manifest_files = manifest_target[DefaultInfo].files.to_list()
        if manifest_files:
            manifest = manifest_files[0]

    if not srcs and manifest == None:
        return [LintModuleInfo(modules = dep_modules, labels = dep_labels, coordinates = coordinates)]

    own_jars = []
    classpath = depset()
    if JavaInfo in target:
        own_jars = target[JavaInfo].compile_jars.to_list()
        classpath = target[JavaInfo].transitive_compile_time_jars
    module = struct(
        label = str(target.label).lstrip("@"),
        package = getattr(ctx.rule.attr, "custom_package", "") or "",
        android = manifest != None or StarlarkAndroidResourcesInfo in target,
        srcs = srcs,
        resources = _module_files(getattr(ctx.rule.attr, "resource_files", [])),
        manifest = manifest,
        own_jars = own_jars,
        classpath = classpath,
        module_deps = dep_labels,
    )
    return [LintModuleInfo(
        modules = dep_modules | {module.label: module},
        labels = depset([module.label], transitive = [dep_labels]),
        coordinates = coordinates,
    )]

_lint_aspect = aspect(
    attr_aspects = ["deps", "exports"],
    implementation = _lint_aspect_impl,
)

def _safe(label):
    return label.replace("@", "").replace("//", "").replace("/", "_").replace(":", "_")

def _paths(files, path_of):
    return [path_of(f) for f in files]

def _module_record(module, app_label, partial, path_of):
    return {
        "label": module.label,
        "package": module.package,
        "android": module.android,
        "app": module.label == app_label,
        "srcs": _paths(module.srcs, path_of),
        "generated_srcs": [path_of(f) for f in module.srcs if not f.is_source],
        "resources": _paths(module.resources, path_of),
        "manifest": path_of(module.manifest) if module.manifest != None else None,
        "jars": _paths(module.own_jars, path_of),
        "partial": path_of(partial) if partial != None else None,
        "deps": sorted(module.module_deps.to_list()),
    }

def _module_spec(module, modules, app_label, partials, path_of, with_partials, coordinates):
    dep_labels = module.module_deps.to_list()
    module_jars = {}
    for label in dep_labels:
        for jar in modules[label].own_jars:
            module_jars[jar.path] = True
    for jar in module.own_jars:
        module_jars[jar.path] = True
    external = [
        jar
        for jar in module.classpath.to_list()
        if jar.path not in module_jars and "/platforms/android-" not in jar.path
    ]
    return {
        "module": _module_record(module, app_label, partials[module.label], path_of),
        "libraries": [
            _module_record(
                modules[label],
                app_label,
                partials[label] if with_partials else None,
                path_of,
            )
            for label in sorted(dep_labels)
        ],
        "external_jars": _paths(external, path_of),
        "coordinates": {path_of(jar): coordinates[jar.path] for jar in external if jar.path in coordinates},
    }

def _module_inputs(module, modules):
    direct = list(module.srcs) + list(module.resources) + list(module.own_jars)
    if module.manifest != None:
        direct.append(module.manifest)
    transitive = [module.classpath]
    for label in module.module_deps.to_list():
        dep = modules[label]
        direct.extend(dep.resources)
        direct.extend(dep.own_jars)
        if dep.manifest != None:
            direct.append(dep.manifest)
    return depset(direct, transitive = transitive)

def _exec_path(f):
    return f.path

def _short_path(f):
    return f.short_path

def _sdk_root(files, path_of):
    for f in files:
        path = path_of(f)
        index = path.find("/sdk/cmdline-tools/")
        if index >= 0:
            return path[:index + len("/sdk")]
    return ""

def _lint_resources(_os, _inputs_size):
    return {"cpu": 2, "memory": 2560}

def _android_lint_impl(ctx):
    modules = {}
    coordinates = {}
    for dep in ctx.attr.deps:
        modules.update(dep[LintModuleInfo].modules)
        for path, coordinate in dep[LintModuleInfo].coordinates.to_list():
            coordinates[path] = coordinate
    app_label = ctx.attr.app_module.lstrip("@")
    if app_label not in modules:
        fail("app_module %s is not reachable from deps" % app_label)

    sdk_files = ctx.files._sdk

    partials = {}
    for label in sorted(modules):
        partials[label] = ctx.actions.declare_directory("%s_partial/%s" % (ctx.label.name, _safe(label)))

    for label in sorted(modules):
        module = modules[label]
        spec = _module_spec(module, modules, app_label, partials, _exec_path, False, coordinates)
        spec["mode"] = "analyze"
        spec["sdk"] = _sdk_root(sdk_files, _exec_path)
        spec["config"] = ctx.file.config.path
        spec_file = ctx.actions.declare_file("%s_spec/%s.analyze.json" % (ctx.label.name, _safe(label)))
        ctx.actions.write(spec_file, json.encode(spec))
        ctx.actions.run(
            executable = ctx.executable._runner,
            arguments = [spec_file.path],
            inputs = depset(
                [spec_file, ctx.file.config] + sdk_files,
                transitive = [_module_inputs(module, modules)],
            ),
            outputs = [partials[label]],
            mnemonic = "AndroidLintAnalyze",
            progress_message = "Android Lint analyze %s" % label,
            resource_set = _lint_resources,
        )

    report_spec = _module_spec(modules[app_label], modules, app_label, partials, _short_path, True, coordinates)
    report_spec["mode"] = "report"
    report_spec["sdk"] = _sdk_root(sdk_files, _short_path)
    report_spec["config"] = ctx.file.config.short_path
    report_file = ctx.actions.declare_file(ctx.label.name + ".report.json")
    ctx.actions.write(report_file, json.encode(report_spec))

    runner = ctx.actions.declare_file(ctx.label.name + ".run")
    ctx.actions.write(runner, "\n".join([
        "#!/usr/bin/env bash",
        "set -euo pipefail",
        "cd \"$TEST_SRCDIR/$TEST_WORKSPACE\"",
        "exec \"%s\" \"%s\"" % (ctx.executable._runner.short_path, report_file.short_path),
        "",
    ]), is_executable = True)

    report_files = [report_file, runner, ctx.file.config] + sdk_files + partials.values()
    for module in modules.values():
        report_files.extend(module.srcs)
        report_files.extend(module.resources)
        if module.manifest != None:
            report_files.append(module.manifest)
    runfiles = ctx.runfiles(files = report_files)
    runfiles = runfiles.merge(ctx.attr._runner[DefaultInfo].default_runfiles)
    return [
        DefaultInfo(executable = runner, runfiles = runfiles),
        OutputGroupInfo(lint_partial = depset(partials.values())),
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
            default = "//platform/android/tools/lint_runner",
            executable = True,
            cfg = "exec",
        ),
        "_sdk": attr.label(
            default = "@android_lint_sdk//:sdk",
            allow_files = True,
        ),
    },
    executable = True,
    test = True,
)

def android_lint(name, deps, app_module, config, **kwargs):
    _android_lint_test(
        name = name,
        deps = deps,
        app_module = app_module,
        config = config,
        **kwargs
    )
