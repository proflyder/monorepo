package dev.proflyder.home.tools.googleservices

import java.io.File
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

private const val CONFIG_FIXTURE = """{
  "project_info": {
    "project_number": "1000000000000",
    "project_id": "example-project",
    "storage_bucket": "example-project.firebasestorage.app"
  },
  "client": [
    {
      "client_info": {
        "mobilesdk_app_id": "1:1000000000000:android:0123456789abcdef",
        "android_client_info": { "package_name": "dev.example.other" }
      },
      "api_key": [ { "current_key": "ExampleKeyOtherClient" } ],
      "oauth_client": []
    },
    {
      "client_info": {
        "mobilesdk_app_id": "1:1000000000000:android:fedcba9876543210",
        "android_client_info": { "package_name": "dev.example.home" }
      },
      "api_key": [ { "current_key": "ExampleKeyHomeClient" } ],
      "oauth_client": []
    }
  ],
  "configuration_version": "1"
}"""

private class Result(val code: Int, val xml: String, val stderr: String)

class GoogleServicesTest {
    private fun runGenerator(
        config: String?,
        packageName: String,
        versionName: String,
        versionCode: String,
    ): Result {
        val temp = Files.createTempDirectory("google-services-").toFile()
        val output = File(temp, "res/values/google_services.xml")
        val args = mutableListOf(
            "--package", packageName,
            "--version-name", versionName,
            "--version-code", versionCode,
            "--output", output.path,
        )
        if (config != null) {
            val input = File(temp, "google-services.json")
            input.writeText(config, Charsets.UTF_8)
            args.add("--input")
            args.add(input.path)
        }
        val stdout = StringBuilder()
        val stderr = StringBuilder()
        val code = run(args.toTypedArray(), stdout, stderr)
        val xml = if (output.exists()) output.readText(Charsets.UTF_8) else ""
        return Result(code, xml, stderr.toString())
    }

    private fun stringValue(xml: String, name: String): String {
        val pattern = Regex(
            "<string name=\"${Regex.escape(name)}\" translatable=\"false\">([^<]*)</string>",
        )
        return pattern.find(xml)?.groupValues?.get(1) ?: ""
    }

    @Test
    fun selectsRequestedPackageAndEmitsGradleEquivalentStrings() {
        val result = runGenerator(CONFIG_FIXTURE, "dev.example.home", "0.1.0", "1")
        assertEquals("exit ${result.code}: ${result.stderr}", 0, result.code)
        val want = mapOf(
            "google_app_id" to "1:1000000000000:android:fedcba9876543210",
            "gcm_defaultSenderId" to "1000000000000",
            "google_api_key" to "ExampleKeyHomeClient",
            "google_crash_reporting_api_key" to "ExampleKeyHomeClient",
            "google_storage_bucket" to "example-project.firebasestorage.app",
            "project_id" to "example-project",
        )
        for ((name, expected) in want) {
            assertEquals(name, expected, stringValue(result.xml, name))
        }
        assertTrue(
            "other client leaked into output: ${result.xml}",
            !result.xml.contains("ExampleKeyOtherClient"),
        )
    }

    @Test
    fun emitsCrashlyticsBuildId() {
        val result = runGenerator(CONFIG_FIXTURE, "dev.example.home", "0.1.0", "1")
        assertEquals("exit ${result.code}: ${result.stderr}", 0, result.code)
        val id = stringValue(result.xml, CRASHLYTICS_BUILD_ID_RESOURCE)
        assertTrue(
            "build id \"$id\" is not 32 lowercase hex chars",
            Regex("^[0-9a-f]{32}$").matches(id),
        )
    }

    @Test
    fun buildIdIsDeterministicAndChangesWithVersion() {
        val first = runGenerator(CONFIG_FIXTURE, "dev.example.home", "0.1.0", "1")
        val second = runGenerator(CONFIG_FIXTURE, "dev.example.home", "0.1.0", "1")
        val third = runGenerator(CONFIG_FIXTURE, "dev.example.home", "0.1.0", "2")
        val name = CRASHLYTICS_BUILD_ID_RESOURCE
        assertEquals(
            "build id is not deterministic",
            stringValue(first.xml, name),
            stringValue(second.xml, name),
        )
        assertNotEquals(
            "build id did not change with version code",
            stringValue(first.xml, name),
            stringValue(third.xml, name),
        )
    }

    @Test
    fun missingPackageIsAnError() {
        val result = runGenerator(CONFIG_FIXTURE, "dev.example.absent", "0.1.0", "1")
        assertTrue("missing package accepted", result.code != 0)
        assertTrue("unexpected stderr: ${result.stderr}", result.stderr.contains("no client for package"))
        assertEquals("output written on error: ${result.xml}", "", result.xml)
    }

    @Test
    fun absentConfigWarnsAndEmitsNoFirebaseStrings() {
        val result = runGenerator(null, "dev.example.home", "0.1.0", "1")
        assertEquals("exit ${result.code}: ${result.stderr}", 0, result.code)
        assertTrue("missing warning: ${result.stderr}", result.stderr.contains("WARNING"))
        for (name in FIREBASE_RESOURCES + CRASHLYTICS_BUILD_ID_RESOURCE) {
            assertTrue("empty config emitted $name: ${result.xml}", !result.xml.contains(name))
        }
        assertTrue("not a values file: ${result.xml}", result.xml.contains("<resources>"))
    }

    @Test
    fun absentFileWithExplicitInputWarns() {
        val temp = Files.createTempDirectory("google-services-").toFile()
        val output = File(temp, "google_services.xml")
        val stdout = StringBuilder()
        val stderr = StringBuilder()
        val code = run(
            arrayOf(
                "--package", "dev.example.home",
                "--input", File(temp, "missing.json").path,
                "--output", output.path,
            ),
            stdout,
            stderr,
        )
        assertEquals("exit $code: $stderr", 0, code)
        assertTrue("missing warning: $stderr", stderr.toString().contains("WARNING"))
    }
}
