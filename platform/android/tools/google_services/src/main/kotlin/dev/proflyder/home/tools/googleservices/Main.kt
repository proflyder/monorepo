// Command google_services generates Android string resources from
// google-services.json.
//
// The Firebase Gradle plugin normally turns `google-services.json` into
// `res/values/google_services.xml` (and a few other resources) at build time.
// The Home Android app is built with Bazel only, so this tool does the same
// subset of that transformation:
//
//   - google_app_id                  <- mobilesdk_app_id
//   - gcm_defaultSenderId            <- project_info.project_number
//   - google_api_key                 <- client.api_key.current_key
//   - google_crash_reporting_api_key <- client.api_key.current_key
//   - google_storage_bucket          <- project_info.storage_bucket
//   - project_id                     <- project_info.project_id
//
// It also emits the Crashlytics build id that the Crashlytics Gradle plugin
// normally writes (`com.google.firebase.crashlytics.mapping_file_id`). The
// Crashlytics SDK only needs this string at init: `AppData` reads
// `com.google.firebase.crashlytics.mapping_file_id`, falling back to the legacy
// `com.crashlytics.android.build_id`, and refuses to start when both are absent.
// The value must be stable for one build and change when the build changes, so
// it is derived from the app id, version name, and version code (32 hex chars).
//
// The JSON is optional in the repository: without it the output is a valid,
// empty values file and a warning goes to stderr, so CI and clean checkouts keep
// building. Firebase then stays uninitialized.
package dev.proflyder.home.tools.googleservices

import java.io.File
import java.security.MessageDigest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

val FIREBASE_RESOURCES = listOf(
    "google_app_id",
    "gcm_defaultSenderId",
    "google_api_key",
    "google_crash_reporting_api_key",
    "google_storage_bucket",
    "project_id",
)

const val CRASHLYTICS_BUILD_ID_RESOURCE = "com.google.firebase.crashlytics.mapping_file_id"

internal val EMPTY_VALUES = """<?xml version="1.0" encoding="utf-8"?>
<resources>
    <!-- google-services.json is not linked; Firebase stays uninitialized. -->
</resources>
"""

/** google-services.json does not describe the requested package. */
internal class ConfigError(val packageName: String) :
    Exception("google-services.json has no client for package '$packageName'")

private fun JsonObject.objectField(key: String): JsonObject? = this[key] as? JsonObject

private fun JsonObject.stringField(key: String): String {
    val value = this[key] as? JsonPrimitive ?: return ""
    return if (value.isString) value.content else ""
}

private fun clientForPackage(config: JsonObject, packageName: String): JsonObject {
    val clients = config["client"] as? JsonArray
    if (clients != null) {
        for (item in clients) {
            val client = item as? JsonObject ?: continue
            val info = client.objectField("client_info") ?: continue
            val android = info.objectField("android_client_info") ?: continue
            if (android.stringField("package_name") == packageName) {
                return client
            }
        }
    }
    throw ConfigError(packageName)
}

internal fun buildId(appId: String, versionName: String, versionCode: String): String {
    val material = listOf(appId, versionName, versionCode).joinToString("\n")
    val digest = MessageDigest.getInstance("SHA-256").digest(material.toByteArray(Charsets.UTF_8))
    return digest.joinToString("") { "%02x".format(it) }.substring(0, 32)
}

// escapeXML mirrors xml.sax.saxutils.escape(value, {'"': "&quot;"}).
internal fun escapeXml(value: String): String {
    val builder = StringBuilder()
    for (ch in value) {
        when (ch) {
            '&' -> builder.append("&amp;")
            '>' -> builder.append("&gt;")
            '<' -> builder.append("&lt;")
            '"' -> builder.append("&quot;")
            else -> builder.append(ch)
        }
    }
    return builder.toString()
}

private fun stringElement(name: String, value: String): String =
    """    <string name="$name" translatable="false">${escapeXml(value)}</string>"""

/**
 * Returns the values XML for one package. An absent or empty input yields an
 * empty values file; a present input without the package is an error.
 */
internal fun generate(
    inputPath: String,
    packageName: String,
    versionName: String,
    versionCode: String,
): String {
    if (inputPath.isEmpty()) {
        return EMPTY_VALUES
    }
    val input = File(inputPath)
    if (!input.exists() || input.length() == 0L) {
        return EMPTY_VALUES
    }

    val payload = input.readText(Charsets.UTF_8)
    val parsed: JsonElement = Json.parseToJsonElement(payload)
    val config = parsed as? JsonObject ?: throw IllegalArgumentException("google-services.json is not an object")

    val projectInfo = config.objectField("project_info") ?: JsonObject(emptyMap())
    val client = clientForPackage(config, packageName)
    val apiKey = ((client["api_key"] as? JsonArray)?.firstOrNull() as? JsonObject)
        ?.stringField("current_key") ?: ""
    val clientInfo = client.objectField("client_info") ?: JsonObject(emptyMap())
    val appId = clientInfo.stringField("mobilesdk_app_id")

    val values = mapOf(
        "google_app_id" to appId,
        "gcm_defaultSenderId" to projectInfo.stringField("project_number"),
        "google_api_key" to apiKey,
        "google_crash_reporting_api_key" to apiKey,
        "google_storage_bucket" to projectInfo.stringField("storage_bucket"),
        "project_id" to projectInfo.stringField("project_id"),
    )

    val lines = mutableListOf("""<?xml version="1.0" encoding="utf-8"?>""", "<resources>")
    for (name in FIREBASE_RESOURCES) {
        lines.add(stringElement(name, values[name] ?: ""))
    }
    lines.add(stringElement(CRASHLYTICS_BUILD_ID_RESOURCE, buildId(appId, versionName, versionCode)))
    lines.add("</resources>")
    return lines.joinToString("\n") + "\n"
}

private fun Appendable.line(text: String) {
    append(text)
    append('\n')
}

private class FlagError(message: String) : Exception(message)

private class ParsedArgs(
    val input: String,
    val packageName: String,
    val versionName: String,
    val versionCode: String,
    val output: String,
)

private fun parseArgs(args: Array<String>): ParsedArgs {
    var input = ""
    var packageName = ""
    var versionName = "0.1.0"
    var versionCode = "1"
    var output = ""
    var index = 0
    while (index < args.size) {
        val arg = args[index]
        if (arg == "--") {
            break
        }
        if (!arg.startsWith("-") || arg == "-") {
            throw FlagError("unexpected argument: $arg")
        }
        val body = arg.trimStart('-')
        val separator = body.indexOf('=')
        val name: String
        val value: String?
        if (separator >= 0) {
            name = body.substring(0, separator)
            value = body.substring(separator + 1)
        } else {
            name = body
            value = null
        }
        if (name !in setOf("input", "package", "version-name", "version-code", "output")) {
            throw FlagError("flag provided but not defined: -$name")
        }
        val resolved = value ?: run {
            if (index + 1 >= args.size) {
                throw FlagError("flag needs an argument: -$name")
            }
            index++
            args[index]
        }
        when (name) {
            "input" -> input = resolved
            "package" -> packageName = resolved
            "version-name" -> versionName = resolved
            "version-code" -> versionCode = resolved
            "output" -> output = resolved
        }
        index++
    }
    return ParsedArgs(input, packageName, versionName, versionCode, output)
}

private const val MISSING_WARNING =
    "WARNING: google-services.json is not linked; generating an empty " +
        "values file and leaving Firebase uninitialized. Link " +
        "platform/android/keys/google-services.json to enable Crashlytics.\n"

fun run(args: Array<String>, stdout: Appendable, stderr: Appendable): Int {
    val parsed = try {
        parseArgs(args)
    } catch (error: FlagError) {
        stderr.line(error.message ?: error.toString())
        return 2
    }
    if (parsed.packageName.isEmpty() || parsed.output.isEmpty()) {
        stderr.line("google_services: --package and --output are required")
        return 2
    }

    if (parsed.input.isEmpty()) {
        stderr.append(MISSING_WARNING)
    } else if (!File(parsed.input).exists()) {
        stderr.append(MISSING_WARNING)
    }

    val xml = try {
        generate(parsed.input, parsed.packageName, parsed.versionName, parsed.versionCode)
    } catch (error: Exception) {
        stderr.line("ERROR: ${error.message ?: error.toString()}")
        return 1
    }

    val outputFile = File(parsed.output)
    val directory = outputFile.parentFile
    if (directory != null && directory.path != "." && directory.path.isNotEmpty()) {
        if (!directory.exists() && !directory.mkdirs()) {
            stderr.line("ERROR: cannot create ${directory.path}")
            return 1
        }
    }
    try {
        outputFile.writeText(xml, Charsets.UTF_8)
    } catch (error: Exception) {
        stderr.line("ERROR: ${error.message ?: error.toString()}")
        return 1
    }
    return 0
}

fun main(args: Array<String>) {
    System.exit(run(args, System.out, System.err))
}
