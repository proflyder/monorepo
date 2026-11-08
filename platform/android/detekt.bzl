"""Bazel-driven detekt for the Home Android client.

`rules_android`/`rules_kotlin` have no detekt rule, so this wraps the detekt CLI
from Maven. An aspect walks the target graph once and collects every first-party
Kotlin source file; the `detekt` rule writes that list and runs the CLI with the
project config and the first-party `home-rules` plug-in. A finding fails the
test, mirroring `android_lint`, so CI treats both checks the same way.
"""

DetektSourcesInfo = provider(
    doc = "Kotlin sources of a target and everything it depends on.",
    fields = {
        "srcs": "depset of .kt/.kts files",
    },
)

def _kotlin_srcs(ctx):
    files = []
    for src in getattr(ctx.rule.attr, "srcs", []):
        for file in src[DefaultInfo].files.to_list():
            # Generated sources (for example view binding) mirror XML layouts;
            # scanning them would report findings in code nobody wrote.
            if not file.is_source:
                continue
            if file.extension in ("kt", "kts"):
                files.append(file)
    return files

def _detekt_aspect_impl(target, ctx):
    transitive = [depset(_kotlin_srcs(ctx))]
    for dep in getattr(ctx.rule.attr, "deps", []):
        if DetektSourcesInfo in dep:
            transitive.append(dep[DetektSourcesInfo].srcs)
    return [DetektSourcesInfo(srcs = depset(transitive = transitive))]

_detekt_aspect = aspect(
    attr_aspects = ["deps"],
    implementation = _detekt_aspect_impl,
)

def _detekt_impl(ctx):
    transitive = []
    for dep in ctx.attr.deps:
        if DetektSourcesInfo in dep:
            transitive.append(dep[DetektSourcesInfo].srcs)
    sources = [
        file
        for file in depset(transitive = transitive).to_list()
        if not ctx.attr.source_prefix or file.short_path.startswith(ctx.attr.source_prefix)
    ]
    sources = sorted(sources, key = lambda file: file.short_path)
    if not sources:
        fail("detekt: no Kotlin sources reachable from %s" % ctx.attr.deps)

    source_list = ctx.actions.declare_file(ctx.label.name + ".sources")
    ctx.actions.write(
        source_list,
        "\n".join([file.short_path for file in sources]) + "\n",
    )

    plugin = [file for file in ctx.attr.plugins[DefaultInfo].files.to_list() if file.extension == "jar"]
    if not plugin:
        fail("detekt: plugins target %s produced no jar" % ctx.attr.plugins.label)

    runner = ctx.actions.declare_file(ctx.label.name + ".run")
    ctx.actions.write(
        runner,
        "\n".join([
            "#!/usr/bin/env bash",
            "set -euo pipefail",
            "cd \"$TEST_SRCDIR/$TEST_WORKSPACE\"",
            "input=\"$(paste -sd, \"%s\")\"" % source_list.short_path,
            "exec \"$TEST_SRCDIR/$TEST_WORKSPACE/%s\" \\" % ctx.executable._detekt.short_path,
            "  --input \"$input\" \\",
            "  --config \"%s\" \\" % ctx.file.config.short_path,
            "  --plugins \"%s\" \\" % plugin[0].short_path,
            "  --base-path \"$TEST_SRCDIR/$TEST_WORKSPACE\" \\",
            "  --jvm-target 17",
        ]) + "\n",
        is_executable = True,
    )

    runfiles = ctx.runfiles(
        files = sources + [
            source_list,
            runner,
            ctx.file.config,
            plugin[0],
        ],
    )
    runfiles = runfiles.merge(ctx.attr._detekt[DefaultInfo].default_runfiles)
    return [
        DefaultInfo(executable = runner, runfiles = runfiles),
        RunEnvironmentInfo(
            inherited_environment = [
                "JAVA_HOME",
                "PATH",
            ],
        ),
    ]

_detekt_test = rule(
    implementation = _detekt_impl,
    attrs = {
        "deps": attr.label_list(aspects = [_detekt_aspect]),
        "config": attr.label(
            doc = "detekt.yml configuration file.",
            allow_single_file = True,
        ),
        "plugins": attr.label(
            doc = "Jar providing the `home-rules` RuleSetProvider.",
            mandatory = True,
        ),
        "source_prefix": attr.string(
            doc = "Only scan sources whose workspace path starts with this prefix.",
        ),
        "_detekt": attr.label(
            default = "//platform/android/tools/detekt",
            executable = True,
            cfg = "exec",
        ),
    },
    executable = True,
    test = True,
)

def home_detekt(
        name,
        deps,
        config,
        plugins = "//platform/android/detekt-rules:detekt-rules",
        source_prefix = ""):
    """Run detekt over every Kotlin source reachable from `deps`."""
    _detekt_test(
        name = name,
        deps = deps,
        config = config,
        plugins = plugins,
        source_prefix = source_prefix,
    )
