package dev.proflyder.home.tools.lintrunner

import java.io.ByteArrayOutputStream
import java.io.File
import java.nio.file.Files
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

private const val MIN_SDK = "26"
private const val TARGET_SDK = "36"
private const val COMPILE_TARGET = "android-36"
private const val VERSION_CODE = "1"
private const val VERSION_NAME = "0.1.0"
private const val VARIANT = "main"
private const val ARTIFACT = "artifact"
private const val ROOT_VARIABLE = "ROOT"
private const val LINT_TOOLS = "cmdline-tools/23.0"
private const val LINT_HEAP = "-Xmx2g"
private const val REPORT_XML = "report.xml"

private const val ANDROID_NS = "http://schemas.android.com/apk/res/android"

private val PACKAGE_PATTERN = Regex("(?m)^package\\s+([\\w.]+)")

enum class Mode { ANALYZE, REPORT }

data class LintModule(
    val label: String,
    val packageName: String,
    val android: Boolean,
    val app: Boolean,
    val srcs: List<String>,
    val generatedSrcs: List<String>,
    val resources: List<String>,
    val manifest: String?,
    val jars: List<String>,
    val partial: String?,
    val deps: List<String>,
)

data class LintSpec(
    val mode: Mode,
    val sdk: String,
    val config: String,
    val module: LintModule,
    val libraries: List<LintModule>,
    val externalJars: List<String>,
    val coordinates: Map<String, String>,
)

private class Element(val tag: String) {
    val attrs = mutableListOf<Pair<String, String>>()
    val children = mutableListOf<Element>()

    fun attr(name: String, value: String): Element {
        attrs.add(name to value)
        return this
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

private fun writeElement(element: Element, builder: StringBuilder, indent: String) {
    builder.append(indent).append('<').append(element.tag)
    for ((name, value) in element.attrs) {
        builder.append(' ').append(name).append("=\"").append(escapeAttr(value)).append('"')
    }
    if (element.children.isEmpty()) {
        builder.append(" />\n")
        return
    }
    builder.append(">\n")
    for (child in element.children) {
        writeElement(child, builder, "$indent  ")
    }
    builder.append(indent).append("</").append(element.tag).append(">\n")
}

private fun writeDocument(file: File, root: Element) {
    val builder = StringBuilder("<?xml version='1.0' encoding='utf-8'?>\n")
    writeElement(root, builder, "")
    file.parentFile?.mkdirs()
    file.writeText(builder.toString(), Charsets.UTF_8)
}

fun safeName(name: String): String {
    val builder = StringBuilder()
    for (ch in name) {
        builder.append(if (ch.isLetterOrDigit() || ch == '.' || ch == '_' || ch == '-') ch else '_')
    }
    return builder.toString().trim('_')
}

fun modulePath(label: String): String =
    ":" + label.removePrefix("@").removePrefix("//").replace('/', ':').trim(':')

fun libraryCoordinate(label: String): String = "bazel.module:${safeName(label)}:unspecified"

fun jarCoordinate(path: String, coordinates: Map<String, String>): String =
    coordinates[path] ?: "bazel.jar:${safeName(path)}:unspecified"

fun sourceRoot(path: String, text: String): String {
    val file = File(path)
    val parent = file.parentFile?.path ?: "."
    val packageName = PACKAGE_PATTERN.find(text)?.groupValues?.get(1) ?: return parent
    val suffix = File.separator + packageName.replace('.', File.separatorChar)
    return if (parent.endsWith(suffix)) parent.removeSuffix(suffix) else parent
}

fun minimalRoots(roots: Collection<String>): List<String> {
    val sorted = roots.toSortedSet().toList()
    val result = mutableListOf<String>()
    for (root in sorted) {
        if (result.none { root == it || root.startsWith(it + File.separator) }) {
            result.add(root)
        }
    }
    return result
}

fun resourceRoots(resources: List<String>): List<String> =
    resources.mapNotNull { File(it).parentFile?.parentFile?.path }.toSortedSet().toList()

private fun JsonObject.string(key: String): String? {
    val value = this[key]
    if (value == null || value is JsonNull) {
        return null
    }
    return (value as? JsonPrimitive)?.takeIf { it.isString }?.content
}

private fun JsonObject.flag(key: String): Boolean = (this[key] as? JsonPrimitive)?.content == "true"

private fun JsonObject.strings(key: String): List<String> =
    (this[key] as? JsonArray)?.mapNotNull { (it as? JsonPrimitive)?.takeIf { p -> p.isString }?.content }
        ?: emptyList()

private fun readModule(element: JsonElement): LintModule {
    val record = element as? JsonObject ?: throw IllegalArgumentException("module record is not an object")
    return LintModule(
        label = record.string("label") ?: throw IllegalArgumentException("module without label"),
        packageName = record.string("package") ?: "",
        android = record.flag("android"),
        app = record.flag("app"),
        srcs = record.strings("srcs"),
        generatedSrcs = record.strings("generated_srcs"),
        resources = record.strings("resources"),
        manifest = record.string("manifest"),
        jars = record.strings("jars"),
        partial = record.string("partial"),
        deps = record.strings("deps"),
    )
}

fun readSpec(text: String): LintSpec {
    val root = Json.parseToJsonElement(text) as? JsonObject
        ?: throw IllegalArgumentException("lint spec is not a JSON object")
    val mode = when (root.string("mode")) {
        "analyze" -> Mode.ANALYZE
        "report" -> Mode.REPORT
        else -> throw IllegalArgumentException("lint spec mode must be analyze or report")
    }
    return LintSpec(
        mode = mode,
        sdk = root.string("sdk") ?: "",
        config = root.string("config") ?: throw IllegalArgumentException("lint spec without config"),
        module = readModule(root["module"] ?: throw IllegalArgumentException("lint spec without module")),
        libraries = (root["libraries"] as? JsonArray)?.map(::readModule) ?: emptyList(),
        externalJars = root.strings("external_jars"),
        coordinates = (root["coordinates"] as? JsonObject)
            ?.mapNotNull { (key, value) -> (value as? JsonPrimitive)?.content?.let { key to it } }
            ?.toMap()
            ?: emptyMap(),
    )
}

class ModelWriter(private val root: File, private val work: File) {
    private fun abs(path: String): String = if (File(path).isAbsolute) path else File(root, path).path

    private fun mergedManifest(module: LintModule): File {
        val file = File(work, "manifests/${safeName(module.label)}.xml")
        file.parentFile.mkdirs()
        if (module.app && module.manifest != null) {
            file.writeText(appManifest(File(abs(module.manifest)).readText(Charsets.UTF_8)), Charsets.UTF_8)
            return file
        }
        val manifest = Element("manifest")
            .attr("xmlns:android", ANDROID_NS)
            .attr("package", module.packageName)
        manifest.child("uses-sdk")
            .attr("android:minSdkVersion", MIN_SDK)
            .attr("android:targetSdkVersion", TARGET_SDK)
        writeDocument(file, manifest)
        return file
    }

    private fun stagedGenerated(module: LintModule): String? {
        if (module.generatedSrcs.isEmpty()) {
            return null
        }
        val base = File(work, "generated/${safeName(module.label)}")
        for (path in module.generatedSrcs) {
            val payload = File(abs(path)).readBytes()
            val packageName = PACKAGE_PATTERN.find(String(payload, Charsets.UTF_8))?.groupValues?.get(1)
            val directory = if (packageName == null) base else File(base, packageName.replace('.', File.separatorChar))
            directory.mkdirs()
            File(directory, File(path).name).writeBytes(payload)
        }
        return base.path
    }

    private fun library(module: LintModule): Element {
        val element = Element("library")
            .attr("name", libraryCoordinate(module.label))
            .attr("jars", existing(module.jars).joinToString(File.pathSeparator))
            .attr("resolved", libraryCoordinate(module.label))
        if (module.android) {
            val folder = File(work, "libraries/${safeName(module.label)}")
            folder.mkdirs()
            val resRoots = resourceRoots(module.resources)
            if (resRoots.size > 1) {
                throw IllegalStateException("${module.label} has several resource roots: $resRoots")
            }
            element.attr("folder", folder.path)
            element.attr("resFolder", resRoots.firstOrNull()?.let(::abs) ?: File(folder, "res").path)
            element.attr("assetsFolder", File(folder, "assets").path)
            element.attr("lintJar", File(folder, "lint.jar").path)
            element.attr("publicResources", File(folder, "public.txt").path)
            element.attr("symbolFile", File(folder, "R.txt").path)
            element.attr("manifest", mergedManifest(module).path)
            element.attr("externalAnnotations", File(folder, "annotations.zip").path)
            element.attr("proguardRules", File(folder, "proguard.txt").path)
        }
        return element
    }

    private fun moduleReference(module: LintModule): Element =
        Element("library")
            .attr("name", modulePath(module.label))
            .attr("project", modulePath(module.label))

    private fun existing(paths: List<String>): List<String> = paths.map(::abs).filter { File(it).exists() }

    private fun externalLibraries(spec: LintSpec, taken: MutableSet<String>): List<Element> {
        val result = mutableListOf<Element>()
        val seenJars = mutableSetOf<String>()
        for (jar in spec.externalJars) {
            val path = abs(jar)
            if (!File(path).exists() || !seenJars.add(path)) {
                continue
            }
            var name = jarCoordinate(jar, spec.coordinates)
            if (!taken.add(name)) {
                name = "bazel.jar:${safeName(jar)}:unspecified"
                taken.add(name)
            }
            result.add(Element("library").attr("name", name).attr("jars", path).attr("resolved", name))
        }
        return result
    }

    fun write(spec: LintSpec): List<File> {
        val bootClassPath = File(abs(spec.sdk), "platforms/$COMPILE_TARGET/android.jar").path
        if (spec.mode == Mode.ANALYZE) {
            val libraries = spec.libraries.map(::library).toMutableList()
            val taken = spec.libraries.map { libraryCoordinate(it.label) }.toMutableSet()
            libraries.addAll(externalLibraries(spec, taken))
            return listOf(writeModel(spec.module, libraries, bootClassPath))
        }
        val byLabel = spec.libraries.associateBy { it.label }
        val models = mutableListOf<File>()
        for (module in listOf(spec.module) + spec.libraries) {
            val libraries = module.deps.mapNotNull { byLabel[it] }.map(::moduleReference).toMutableList()
            if (module.app) {
                val taken = libraries.map { it.attrs.first { (key, _) -> key == "name" }.second }.toMutableSet()
                libraries.addAll(externalLibraries(spec, taken))
            }
            models.add(writeModel(module, libraries, bootClassPath))
        }
        return models
    }

    private fun writeModel(module: LintModule, libraryElements: List<Element>, bootClassPath: String): File {
        val modelDir = File(work, "models/${safeName(module.label)}")
        modelDir.mkdirs()
        val moduleDir = File(work, "modules/${safeName(module.label)}")
        moduleDir.mkdirs()
        val type = when {
            module.app -> "APP"
            module.android -> "LIBRARY"
            else -> "JAVA_LIBRARY"
        }

        val lintModule = Element("lint-module")
            .attr("format", "1")
            .attr("dir", moduleDir.path)
            .attr("name", modulePath(module.label))
            .attr("type", type)
            .attr("maven", libraryCoordinate(module.label))
            .attr("buildFolder", File(work, "build/${safeName(module.label)}").path)
            .attr("bootClassPath", bootClassPath)
            .attr("javaSourceLevel", "17")
            .attr("compileTarget", COMPILE_TARGET)
            .attr("neverShrinking", "true")
        lintModule.child("lintOptions")
            .attr("absolutePaths", "true")
            .attr("checkDependencies", if (module.app) "true" else "false")
            .attr("checkReleaseBuilds", "true")
            .attr("checkGeneratedSources", "true")
        lintModule.child("variant").attr("name", VARIANT)
        writeDocument(File(modelDir, "module.xml"), lintModule)

        val sourceRoots = minimalRoots(
            module.srcs.filter { it !in module.generatedSrcs }.map {
                abs(sourceRoot(it, File(abs(it)).readText(Charsets.UTF_8)))
            },
        )
        val resRoots = resourceRoots(module.resources).map(::abs)
        val declaresSdk = module.manifest != null &&
            File(abs(module.manifest)).readText(Charsets.UTF_8).contains("<uses-sdk")
        val variant = Element("variant").attr("name", VARIANT)
        if (!declaresSdk) {
            variant.attr("minSdkVersion", MIN_SDK)
            variant.attr("targetSdkVersion", TARGET_SDK)
        }
        variant.attr("debuggable", "false")
        variant.attr("mergedManifest", mergedManifest(module).path)
        if (module.packageName.isNotEmpty()) {
            variant.attr("package", module.packageName)
        }
        if (module.partial != null) {
            variant.attr("partialResultsDir", abs(module.partial))
        }
        val provider = variant.child("sourceProviders").child("sourceProvider")
        if (module.manifest != null) {
            provider.attr("manifests", abs(module.manifest))
        }
        provider.attr("javaDirectories", sourceRoots.joinToString(File.pathSeparator))
        provider.attr("resDirectories", resRoots.joinToString(File.pathSeparator))
        variant.child("testSourceProviders")
        variant.child("testFixturesSourceProviders")
        variant.child("resValues")
        variant.child("manifestPlaceholders")
        variant.child("buildFeatures")
            .attr("coreLibraryDesugaring", "false")
            .attr("viewBinding", if (module.generatedSrcs.isEmpty()) "false" else "true")
        val artifact = variant.child(ARTIFACT)
            .attr("classOutputs", existing(module.jars).joinToString(File.pathSeparator))
            .attr("type", "MAIN")
            .attr("applicationId", module.packageName.ifEmpty { safeName(module.label) })
        stagedGenerated(module)?.let { artifact.attr("generatedSourceFolders", it) }
        writeDocument(File(modelDir, "$VARIANT.xml"), variant)

        val libraries = Element("libraries")
        libraries.children.addAll(libraryElements)
        writeDocument(File(modelDir, "$VARIANT-$ARTIFACT-libraries.xml"), libraries)

        val names = libraryElements.map { element -> element.attrs.first { it.first == "name" }.second }
        val dependencies = Element("dependencies")
        for (graph in listOf("compile", "package")) {
            val element = dependencies.child(graph).attr("roots", names.joinToString(","))
            for (name in names) {
                element.child("dependency")
                    .attr("name", name)
                    .attr("simpleName", name.substringBeforeLast(':'))
            }
        }
        writeDocument(File(modelDir, "$VARIANT-$ARTIFACT-dependencies.xml"), dependencies)
        return modelDir
    }
}

fun appManifest(text: String): String {
    val open = Regex("<manifest\\b").find(text) ?: throw IllegalArgumentException("application manifest has no <manifest>")
    val extra = StringBuilder()
    if (!text.contains("android:versionCode")) {
        extra.append(" android:versionCode=\"").append(VERSION_CODE).append('"')
    }
    if (!text.contains("android:versionName")) {
        extra.append(" android:versionName=\"").append(VERSION_NAME).append('"')
    }
    val at = open.range.last + 1
    return text.substring(0, at) + extra + text.substring(at)
}

fun countFindings(xml: String): Int =
    Regex("<issue\\b[^>]*\\bseverity=\"(Warning|Error|Fatal)\"").findAll(xml).count()

fun lintCommand(spec: LintSpec, root: File, models: List<File>, work: File, javaHome: String): List<String> {
    val sdk = File(root, spec.sdk).absoluteFile
    val tools = File(sdk, LINT_TOOLS)
    val command = mutableListOf(
        File(javaHome, "bin/java").path,
        LINT_HEAP,
        "-Duser.home=${work.path}",
        "-Dcom.android.tools.lint.bindir=${tools.path}",
        "-classpath",
        File(tools, "lib/lint-classpath.jar").path,
        "com.android.tools.lint.Main",
        "--sdk-home", sdk.path,
        "--config", File(root, spec.config).path,
        "--path-variables", "$ROOT_VARIABLE=${root.path}",
        "--cache-dir", File(work, "cache").path,
    )
    for (model in models) {
        command.add("--lint-model")
        command.add(model.path)
    }
    command.addAll(listOf("--variant", VARIANT, "--quiet"))
    when (spec.mode) {
        Mode.ANALYZE -> command.add("--analyze-only")
        Mode.REPORT -> command.addAll(
            listOf("--report-only", "-Werror", "--text", "stdout", "--xml", File(work, REPORT_XML).path),
        )
    }
    return command
}

private fun Appendable.line(text: String) {
    append(text).append('\n')
}

fun run(args: Array<String>, stdout: Appendable, stderr: Appendable): Int {
    if (args.size != 1) {
        stderr.line("usage: lint_runner <spec.json>")
        return 2
    }
    val root = File("").absoluteFile
    val spec = try {
        readSpec(File(args[0]).readText(Charsets.UTF_8))
    } catch (error: Exception) {
        stderr.line(error.message ?: error.toString())
        return 1
    }
    if (spec.sdk.isEmpty() || !File(root, "${spec.sdk}/$LINT_TOOLS/lib/lint-classpath.jar").exists()) {
        stderr.line(
            "Android Lint was not found. Install `cmdline-tools;23.0` and `platforms;android-36` " +
                "into ANDROID_HOME (sdkmanager --install \"cmdline-tools;23.0\") and rerun bazel.",
        )
        return 1
    }
    val work = Files.createTempDirectory("android-lint-").toFile()
    try {
        val models = ModelWriter(root, work).write(spec)
        if (spec.mode == Mode.ANALYZE) {
            spec.module.partial?.let { File(root, it).mkdirs() }
        }
        val command = lintCommand(spec, root, models, work, System.getProperty("java.home"))
        val process = ProcessBuilder(command).apply {
            environment()["ANDROID_USER_HOME"] = File(work, "android-user").path
            environment().remove("ANDROID_LINT_JARS")
        }.start()
        val outBuffer = ByteArrayOutputStream()
        val errBuffer = ByteArrayOutputStream()
        val outThread = Thread { process.inputStream.copyTo(outBuffer) }
        val errThread = Thread { process.errorStream.copyTo(errBuffer) }
        outThread.start()
        errThread.start()
        val exit = process.waitFor()
        outThread.join()
        errThread.join()
        stdout.append(outBuffer.toString(Charsets.UTF_8.name()))
        stderr.append(errBuffer.toString(Charsets.UTF_8.name()))
        if (exit != 0 || spec.mode == Mode.ANALYZE) {
            return exit
        }
        val report = File(work, REPORT_XML)
        if (!report.exists()) {
            stderr.line("Android Lint produced no report")
            return 1
        }
        val errors = countFindings(report.readText(Charsets.UTF_8))
        if (errors > 0) {
            stderr.line("Android Lint found $errors finding(s); warnings fail like errors (-Werror)")
            return 1
        }
        return 0
    } catch (error: Exception) {
        stderr.line(error.message ?: error.toString())
        return 1
    } finally {
        work.deleteRecursively()
    }
}

fun main(args: Array<String>) {
    System.exit(run(args, System.out, System.err))
}
