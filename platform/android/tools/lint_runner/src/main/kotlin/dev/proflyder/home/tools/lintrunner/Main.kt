// Command lint_runner runs Android Lint over the Bazel modules of the Home
// Android client.
//
// The Bazel rule (`android_lint` in lint.bzl) writes one JSON record per module
// with the runfiles-relative paths of its sources, resources and manifest. This
// tool turns those records into the `project.xml` layout descriptor understood
// by the standalone Android Lint CLI and runs lint once over the whole project,
// so cross-module resource usage is analysed as a single build rather than as
// unrelated modules.
//
// The lint tool is not a Bazel input: it ships with the Android SDK
// `cmdline-tools` package. The Bazel rule sets ANDROID_HOME/ANDROID_SDK_ROOT in
// the test environment; CI installs the pinned `cmdline-tools;23.0`.
package dev.proflyder.home.tools.lintrunner

import java.io.ByteArrayOutputStream
import java.io.File
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import java.nio.file.attribute.PosixFilePermission
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.JsonNull

private const val MIN_SDK = "26"
private const val TARGET_SDK = "36"
private const val COMPILE_SDK = "36"
private const val VERSION_CODE = "1"
private const val VERSION_NAME = "0.1.0"

private const val ANDROID_NS = "http://schemas.android.com/apk/res/android"

private val PACKAGE_PATTERN = Regex("(?m)^package\\s+([\\w.]+)")

/** Environment access seam so tests never mutate the JVM process environment. */
interface Env {
    fun get(key: String): String?
    fun entries(): List<Pair<String, String>>
}

object SystemEnvironment : Env {
    override fun get(key: String): String? = System.getenv(key)

    override fun entries(): List<Pair<String, String>> = System.getenv().map { it.key to it.value }
}

/** Mutable isolated environment for tests and embedded runs. */
class MapEnvironment : Env {
    private val values = LinkedHashMap<String, String>()

    fun set(key: String, value: String): MapEnvironment {
        values[key] = value
        return this
    }

    fun remove(key: String) {
        values.remove(key)
    }

    override fun get(key: String): String? = values[key]

    override fun entries(): List<Pair<String, String>> = values.map { it.key to it.value }
}

/** One module record written by the Bazel lint aspect. */
data class LintModule(
    val label: String,
    val name: String,
    val packageName: String,
    val android: Boolean,
    val app: Boolean,
    val srcs: List<String>,
    val generatedSrcs: List<String>,
    val resources: List<String>,
    val manifest: String?,
    val deps: List<String>,
    val jars: List<String>,
)

private class Attribute(var name: String, var value: String)

private class Element(val tag: String) {
    val attrs = mutableListOf<Attribute>()
    val children = mutableListOf<Element>()

    fun attr(name: String, value: String) {
        attrs.add(Attribute(name, value))
    }

    fun child(tag: String): Element {
        val child = Element(tag)
        children.add(child)
        return child
    }
}

private fun escapeAttr(value: String): String {
    val builder = StringBuilder()
    for (ch in value) {
        when (ch) {
            '&' -> builder.append("&amp;")
            '<' -> builder.append("&lt;")
            '>' -> builder.append("&gt;")
            '"' -> builder.append("&quot;")
            '\r' -> builder.append("&#13;")
            '\n' -> builder.append("&#10;")
            '\t' -> builder.append("&#09;")
            else -> builder.append(ch)
        }
    }
    return builder.toString()
}

/** Separates ElementTree-style `{uri}local` names. */
private fun splitQName(name: String): Pair<String, String> {
    if (name.startsWith("{")) {
        val end = name.indexOf('}')
        if (end >= 0) {
            return name.substring(1, end) to name.substring(end + 1)
        }
    }
    return "" to name
}

/**
 * Assigns ns0, ns1, ... to namespaced attributes in document order, mirroring
 * ElementTree's serialization of the lint model.
 */
private fun collectNamespaces(
    element: Element,
    prefixes: MutableMap<String, String>,
    order: MutableList<String>,
) {
    for (attr in element.attrs) {
        val uri = splitQName(attr.name).first
        if (uri.isEmpty()) {
            continue
        }
        if (!prefixes.containsKey(uri)) {
            prefixes[uri] = "ns${order.size}"
            order.add(uri)
        }
    }
    for (child in element.children) {
        collectNamespaces(child, prefixes, order)
    }
}

private fun writeElement(
    element: Element,
    builder: StringBuilder,
    prefixes: Map<String, String>,
    order: List<String>,
    isRoot: Boolean,
) {
    val (tagUri, tagLocal) = splitQName(element.tag)
    val tag = if (tagUri.isNotEmpty()) "${prefixes.getValue(tagUri)}:$tagLocal" else tagLocal
    builder.append("<")
    builder.append(tag)
    if (isRoot) {
        for ((index, uri) in order.withIndex()) {
            builder.append(" ")
            builder.append("xmlns:ns$index")
            builder.append("=\"")
            builder.append(escapeAttr(uri))
            builder.append("\"")
        }
    }
    for (attr in element.attrs) {
        val (attrUri, attrLocal) = splitQName(attr.name)
        val name = if (attrUri.isNotEmpty()) "${prefixes.getValue(attrUri)}:$attrLocal" else attr.name
        builder.append(" ")
        builder.append(name)
        builder.append("=\"")
        builder.append(escapeAttr(attr.value))
        builder.append("\"")
    }
    if (element.children.isEmpty()) {
        builder.append(" />")
        return
    }
    builder.append(">")
    for (child in element.children) {
        writeElement(child, builder, prefixes, order, false)
    }
    builder.append("</")
    builder.append(tag)
    builder.append(">")
}

private fun writeDocument(path: String, root: Element) {
    val prefixes = mutableMapOf<String, String>()
    val order = mutableListOf<String>()
    collectNamespaces(root, prefixes, order)

    val builder = StringBuilder()
    builder.append("<?xml version='1.0' encoding='utf-8'?>\n")
    writeElement(root, builder, prefixes, order, true)
    File(path).writeText(builder.toString(), Charsets.UTF_8)
}

// joinPath mirrors os.path.join: it concatenates without cleaning `..`.
// The lint records contain runfiles-relative paths such as
// `../rules_jvm_external++maven+maven/...`; normalising them would produce a
// different (but equivalent) project model.
private fun joinPath(base: String, name: String): String {
    if (File(name).isAbsolute) {
        return name
    }
    if (base.isEmpty()) {
        return name
    }
    val separator = File.separator
    return if (base.endsWith(separator)) base + name else base + separator + name
}

private fun runfilesRoot(env: Env): String {
    val testSrcDir = env.get("TEST_SRCDIR")
    val testWorkspace = env.get("TEST_WORKSPACE")
    if (testSrcDir.isNullOrEmpty() || testWorkspace.isNullOrEmpty()) {
        throw IllegalStateException("TEST_SRCDIR/TEST_WORKSPACE are not set; run through bazel test")
    }
    return joinPath(testSrcDir, testWorkspace)
}

private fun isExecutableFile(path: String): Boolean {
    val file: Path = Paths.get(path)
    if (!Files.exists(file) || Files.isDirectory(file)) {
        return false
    }
    val permissions = try {
        Files.getPosixFilePermissions(file)
    } catch (_: UnsupportedOperationException) {
        return Files.isExecutable(file)
    }
    return permissions.contains(PosixFilePermission.OWNER_EXECUTE) ||
        permissions.contains(PosixFilePermission.GROUP_EXECUTE) ||
        permissions.contains(PosixFilePermission.OTHERS_EXECUTE)
}

private fun sortedDescending(values: List<String>): List<String> {
    val copy = values.toMutableList()
    copy.sort()
    copy.reverse()
    return copy
}

fun findLint(env: Env): String {
    val override = env.get("ANDROID_LINT")
    if (!override.isNullOrEmpty()) {
        return override
    }
    val homes = listOf(env.get("ANDROID_HOME"), env.get("ANDROID_SDK_ROOT"))
    val candidates = mutableListOf<String>()
    for (home in homes) {
        if (home.isNullOrEmpty()) {
            continue
        }
        candidates.add(joinPath(joinPath(joinPath(home, "cmdline-tools"), "23.0"), "bin/lint"))
        candidates.add(joinPath(joinPath(joinPath(home, "cmdline-tools"), "latest"), "bin/lint"))
        val cmdline = File(home, "cmdline-tools")
        val names = cmdline.list()
        if (names != null) {
            for (name in sortedDescending(names.toList())) {
                candidates.add(joinPath(joinPath(joinPath(cmdline.path, name), "bin"), "lint"))
            }
        }
        candidates.add(joinPath(joinPath(joinPath(home, "tools"), "bin"), "lint"))
    }
    for (candidate in candidates) {
        if (isExecutableFile(candidate)) {
            return candidate
        }
    }
    throw IllegalStateException(
        "Android Lint was not found. Install the pinned SDK package " +
            "`cmdline-tools;23.0` (sdkmanager --install \"cmdline-tools;23.0\") " +
            "or set ANDROID_LINT to the lint executable.",
    )
}

fun safeName(name: String): String {
    val builder = StringBuilder()
    for (ch in name) {
        if (ch.isLetter() || ch.isDigit() || ch == '.' || ch == '_' || ch == '-') {
            builder.append(ch)
        } else {
            builder.append('_')
        }
    }
    return builder.toString()
}

/**
 * Copies a generated source into a directory matching its package. Lint warns
 * when a generated file's path does not match its package (the ViewBinding
 * generator emits files into the Bazel package, not the Kotlin package tree).
 * Staging a copy keeps the analysis output clean without changing build
 * outputs.
 */
fun stagedGeneratedSource(item: LintModule, path: String, workdir: String): String {
    val payload = File(path).readBytes()
    val match = PACKAGE_PATTERN.find(String(payload, Charsets.UTF_8)) ?: return path
    val packageName = match.groupValues[1]
    val parts = mutableListOf(workdir, "generated", safeName(item.name))
    parts.addAll(packageName.split("."))
    var directory = File(parts.first())
    for (part in parts.drop(1)) {
        directory = File(directory, part)
    }
    if (!directory.exists() && !directory.mkdirs()) {
        throw IllegalStateException("cannot create ${directory.path}")
    }
    val dest = File(directory, File(path).name)
    dest.writeBytes(payload)
    return dest.path
}

/**
 * Writes a merged manifest carrying the SDK levels Bazel configures. Library
 * source manifests are intentionally empty; the application manifest declares
 * minSdk/targetSdk. Lint reads SDK levels from the merged manifest, so every
 * module gets one. The application module also needs the version fields that
 * android_binary injects through manifest_values.
 */
private fun mergedManifest(item: LintModule, workdir: String): String {
    val manifest = Element("manifest")
    manifest.attr("package", item.packageName)
    val usesSdk = manifest.child("uses-sdk")
    usesSdk.attr("{$ANDROID_NS}minSdkVersion", MIN_SDK)
    usesSdk.attr("{$ANDROID_NS}targetSdkVersion", TARGET_SDK)
    if (item.app) {
        manifest.attr("{$ANDROID_NS}versionCode", VERSION_CODE)
        manifest.attr("{$ANDROID_NS}versionName", VERSION_NAME)
    }

    val path = File(workdir, safeName(item.name) + "-merged-manifest.xml").path
    writeDocument(path, manifest)
    return path
}

private fun JsonObject.stringField(key: String): String {
    val value = this[key] as? JsonPrimitive ?: return ""
    return if (value.isString) value.content else ""
}

private fun JsonObject.listField(key: String): List<String> {
    val array = this[key] as? JsonArray ?: return emptyList()
    return array.mapNotNull { item ->
        val primitive = item as? JsonPrimitive ?: return@mapNotNull null
        if (primitive.isString) primitive.content else null
    }
}

fun readModules(path: String): List<LintModule> {
    val modules = mutableListOf<LintModule>()
    for (line in File(path).readLines(Charsets.UTF_8)) {
        if (line.trim().isEmpty()) {
            continue
        }
        val record = Json.parseToJsonElement(line) as? JsonObject
            ?: throw IllegalArgumentException("module record is not a JSON object: $line")
        val manifestElement = record["manifest"]
        val manifest = if (manifestElement is JsonPrimitive && manifestElement !is JsonNull) {
            record.stringField("manifest")
        } else {
            null
        }
        modules.add(
            LintModule(
                label = record.stringField("label"),
                name = record.stringField("name"),
                packageName = record.stringField("package"),
                android = record["android"]?.let { it is JsonPrimitive && it.content == "true" } ?: false,
                app = record["app"]?.let { it is JsonPrimitive && it.content == "true" } ?: false,
                srcs = record.listField("srcs"),
                generatedSrcs = record.listField("generated_srcs"),
                resources = record.listField("resources"),
                manifest = manifest,
                deps = record.listField("deps"),
                jars = record.listField("jars"),
            ),
        )
    }
    return modules
}

private fun boolString(value: Boolean): String = if (value) "true" else "false"

private fun buildProject(modules: List<LintModule>, root: String, workdir: String): Element {
    val names = modules.map { it.name }.toSet()

    val project = Element("project")
    project.attr("android", "true")
    project.attr("incomplete", "false")
    project.child("root").attr("dir", root)

    for (item in modules) {
        val moduleElement = Element("module")
        moduleElement.attr("name", item.name)
        moduleElement.attr("android", boolString(item.android))
        moduleElement.attr("compile-sdk-version", COMPILE_SDK)
        moduleElement.attr("javaLanguage", "17")
        moduleElement.attr("kotlinLanguage", "2.4")
        if (item.android) {
            moduleElement.attr("library", boolString(!item.app))
        }

        val generated = item.generatedSrcs.toSet()
        for (src in item.srcs) {
            val path = joinPath(root, src)
            val srcElement = Element("src")
            srcElement.attr("file", path)
            if (generated.contains(src)) {
                srcElement.attr("generated", "true")
                val staged = stagedGeneratedSource(item, path, workdir)
                srcElement.attrs[0].value = staged
            }
            moduleElement.children.add(srcElement)
        }
        for (resource in item.resources) {
            moduleElement.child("resource").attr("file", joinPath(root, resource))
        }
        if (item.manifest != null) {
            moduleElement.child("manifest").attr("file", joinPath(root, item.manifest))
            moduleElement.child("merged-manifest").attr("file", mergedManifest(item, workdir))
        }
        for (jar in item.jars) {
            val jarPath = joinPath(root, jar)
            if (File(jarPath).exists()) {
                moduleElement.child("classpath").attr("jar", jarPath)
            }
        }
        for (dep in item.deps) {
            if (names.contains(dep)) {
                moduleElement.child("dep").attr("module", dep)
            }
        }
        project.children.add(moduleElement)
    }
    return project
}

private fun Appendable.line(text: String) {
    append(text)
    append('\n')
}

private class FlagError(message: String) : Exception(message)

private fun positionalArgs(args: Array<String>): List<String> {
    val positional = mutableListOf<String>()
    for (arg in args) {
        if (arg == "--") {
            continue
        }
        if (arg.startsWith("-") && arg != "-") {
            throw FlagError("flag provided but not defined: $arg")
        }
        positional.add(arg)
    }
    return positional
}

fun run(
    args: Array<String>,
    stdout: Appendable,
    stderr: Appendable,
    env: Env = SystemEnvironment,
): Int {
    val positional = try {
        positionalArgs(args)
    } catch (error: FlagError) {
        stderr.line(error.message ?: error.toString())
        return 2
    }

    val recordsPath = when {
        positional.size >= 1 -> positional[0]
        !env.get("LINT_MODULES").isNullOrEmpty() -> env.get("LINT_MODULES")!!
        else -> {
            stderr.line("usage: lint_runner <modules.jsonl> [lint.xml] (or set LINT_MODULES)")
            return 1
        }
    }
    var configPath = if (positional.size >= 2) positional[1] else ""

    val root = try {
        runfilesRoot(env)
    } catch (error: Exception) {
        stderr.line(error.message ?: error.toString())
        return 1
    }
    val resolvedRecords = if (File(recordsPath).isAbsolute) recordsPath else joinPath(root, recordsPath)

    val modules = try {
        readModules(resolvedRecords)
    } catch (error: Exception) {
        stderr.line(error.message ?: error.toString())
        return 1
    }

    val configuredTmp = env.get("TEST_TMPDIR")
    val tmp = if (configuredTmp.isNullOrEmpty()) {
        try {
            Files.createTempDirectory("lint-").toString()
        } catch (error: Exception) {
            stderr.line(error.message ?: error.toString())
            return 1
        }
    } else {
        configuredTmp
    }
    try {
        val directory = File(tmp)
        if (!directory.exists() && !directory.mkdirs()) {
            stderr.line("cannot create $tmp")
            return 1
        }
    } catch (error: Exception) {
        stderr.line(error.message ?: error.toString())
        return 1
    }

    val project = try {
        buildProject(modules, root, tmp)
    } catch (error: Exception) {
        stderr.line(error.message ?: error.toString())
        return 1
    }
    val projectPath = File(tmp, "project.xml").path
    try {
        writeDocument(projectPath, project)
    } catch (error: Exception) {
        stderr.line(error.message ?: error.toString())
        return 1
    }

    if (configPath.isEmpty()) {
        configPath = joinPath(root, "projects/mobile/android/lint.xml")
    }
    val lint = try {
        findLint(env)
    } catch (error: Exception) {
        stderr.line(error.message ?: error.toString())
        return 1
    }

    var sdkHome = env.get("ANDROID_HOME")
    if (sdkHome.isNullOrEmpty()) {
        sdkHome = env.get("ANDROID_SDK_ROOT")
    }
    val command = mutableListOf(
        lint,
        "--project", projectPath,
        "--compile-sdk-version", COMPILE_SDK,
    )
    if (!sdkHome.isNullOrEmpty()) {
        command.add("--sdk-home")
        command.add(sdkHome)
    }
    command.addAll(
        listOf(
            "--config", configPath,
            "--exitcode", "-Werror",
            "--text", "stdout",
            "--quiet",
        ),
    )

    // Rebuild the environment in a deterministic order: inherited entries keep
    // their order, then the two defaults (only when absent).
    val inherited = env.entries()
    val environment = LinkedHashMap<String, String>()
    for ((key, value) in inherited) {
        environment[key] = value
    }
    val extraOrder = mutableListOf<String>()
    fun setDefault(key: String, value: String) {
        if (environment.containsKey(key)) {
            return
        }
        environment[key] = value
        extraOrder.add(key)
    }
    setDefault("ANDROID_USER_HOME", joinPath(tmp, "android-user"))
    setDefault("LINT_OPTS", "-Duser.home=$tmp")

    val commandEnv = mutableListOf<Pair<String, String>>()
    val seen = mutableSetOf<String>()
    for ((key, _) in inherited) {
        if (seen.add(key)) {
            commandEnv.add(key to environment.getValue(key))
        }
    }
    for (key in extraOrder) {
        commandEnv.add(key to environment.getValue(key))
    }

    val process = try {
        ProcessBuilder(command).apply {
            environment().clear()
            for ((key, value) in commandEnv) {
                environment()[key] = value
            }
            redirectInput(ProcessBuilder.Redirect.INHERIT)
        }.start()
    } catch (error: Exception) {
        stderr.line(error.message ?: error.toString())
        return 1
    }

    val outBuffer = ByteArrayOutputStream()
    val errBuffer = ByteArrayOutputStream()
    val outThread = Thread { process.inputStream.copyTo(outBuffer) }
    val errThread = Thread { process.errorStream.copyTo(errBuffer) }
    outThread.start()
    errThread.start()
    val exit = try {
        process.waitFor()
        outThread.join()
        errThread.join()
        process.exitValue()
    } catch (error: InterruptedException) {
        process.destroy()
        Thread.currentThread().interrupt()
        stderr.line(error.message ?: error.toString())
        return 1
    }
    stdout.append(outBuffer.toString(Charsets.UTF_8.name()))
    stderr.append(errBuffer.toString(Charsets.UTF_8.name()))
    return exit
}

fun main(args: Array<String>) {
    System.exit(run(args, System.out, System.err))
}
