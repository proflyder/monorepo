package dev.proflyder.home.tools.lintrunner

import java.io.File
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

private fun writeFile(path: File, content: String) {
    path.parentFile.mkdirs()
    path.writeText(content, Charsets.UTF_8)
}

private class LintFixture(
    val root: File,
    val recordsPath: String,
    val configPath: String,
    val workdir: String,
    val argsFile: String,
    val lintPath: String,
    val sdkHome: String,
    val environment: MapEnvironment,
)

private fun fakeLint(temp: File): String {
    val path = File(temp, "lint")
    path.writeText(
        """#!/bin/sh
printf '%s\n' "${'$'}@" > "${'$'}LINT_ARGS_FILE"
printf 'user_home=%s\n' "${'$'}ANDROID_USER_HOME"
printf 'lint_opts=%s\n' "${'$'}LINT_OPTS"
exit "${'$'}{LINT_EXIT_CODE:-0}"
""",
        Charsets.UTF_8,
    )
    path.setExecutable(true)
    return path.path
}

private fun newLintFixture(): LintFixture {
    val temp = Files.createTempDirectory("lint-runner-").toFile()
    val root = File(temp, "src/ws")
    val workdir = File(temp, "work")
    val argsFile = File(temp, "lint-args.txt")
    val sdkHome = File(temp, "sdk")
    val lintPath = fakeLint(temp)

    val environment = MapEnvironment()
        .set("TEST_SRCDIR", File(temp, "src").path)
        .set("TEST_WORKSPACE", "ws")
        .set("TEST_TMPDIR", workdir.path)
        .set("LINT_ARGS_FILE", argsFile.path)
        .set("ANDROID_LINT", "")
        .set("ANDROID_HOME", sdkHome.path)
        .set("LINT_EXIT_CODE", "")

    writeFile(File(root, "app/ViewBindings.kt"), "package dev.example.databinding\nclass X\n")
    writeFile(File(root, "app/Main.kt"), "package dev.example\n")
    writeFile(File(root, "app/AndroidManifest.xml"), "<manifest />\n")
    writeFile(File(root, "app/res/layout/a.xml"), "<LinearLayout />\n")
    writeFile(File(root, "libs/foo.jar"), "jar")

    val records = listOf(
        """{"label":"//app:app","name":"//app:app","package":"dev.example","android":true,"app":true,""" +
            """"srcs":["app/ViewBindings.kt","app/Main.kt"],"generated_srcs":["app/ViewBindings.kt"],""" +
            """"resources":["app/res/layout/a.xml"],"manifest":"app/AndroidManifest.xml",""" +
            """"deps":["//lib:lib","//absent:absent"],"jars":["libs/foo.jar","libs/missing.jar"]}""",
        """{"label":"//lib:lib","name":"//lib:lib","package":"dev.example.lib","android":true,"app":false}""",
    )
    val recordsPath = File(root, "modules.jsonl")
    writeFile(recordsPath, records.joinToString("\n") + "\n")
    val configPath = File(root, "lint.xml")
    writeFile(configPath, "<lint />\n")

    return LintFixture(
        root = root,
        recordsPath = recordsPath.path,
        configPath = configPath.path,
        workdir = workdir.path,
        argsFile = argsFile.path,
        lintPath = lintPath,
        sdkHome = sdkHome.path,
        environment = environment,
    )
}

private class LintResult(val code: Int, val stdout: String, val stderr: String)

private fun runLint(environment: MapEnvironment, vararg args: String): LintResult {
    val stdout = StringBuilder()
    val stderr = StringBuilder()
    val code = run(arrayOf(*args), stdout, stderr, environment)
    return LintResult(code, stdout.toString(), stderr.toString())
}

class LintRunnerTest {
    @Test
    fun mainFlowBuildsProjectAndRunsLint() {
        val fixture = newLintFixture()
        fixture.environment.set("ANDROID_LINT", fixture.lintPath)

        val result = runLint(fixture.environment, fixture.recordsPath, fixture.configPath)
        assertEquals("exit ${result.code}: ${result.stdout}${result.stderr}", 0, result.code)

        val projectPath = File(fixture.workdir, "project.xml").path
        val text = File(projectPath).readText(Charsets.UTF_8)
        for (want in listOf(
            """<project android="true" incomplete="false">""",
            """<root dir="${fixture.root.path}" />""",
            """<module name="//app:app" android="true" compile-sdk-version="36" javaLanguage="17" """ +
                """kotlinLanguage="2.4" library="false">""",
            """<module name="//lib:lib" android="true" compile-sdk-version="36" javaLanguage="17" """ +
                """kotlinLanguage="2.4" library="true" />""",
            """<resource file="${File(fixture.root, "app/res/layout/a.xml").path}" />""",
            """<manifest file="${File(fixture.root, "app/AndroidManifest.xml").path}" />""",
            """<classpath jar="${File(fixture.root, "libs/foo.jar").path}" />""",
            """<dep module="//lib:lib" />""",
        )) {
            assertTrue("project.xml does not contain \"$want\":\n$text", text.contains(want))
        }
        assertTrue("missing jar was added to the classpath", !text.contains("missing.jar"))
        assertTrue("unknown dependency was added to the module", !text.contains("//absent:absent"))
        val staged = File(fixture.workdir, "generated/__app_app/dev/example/databinding/ViewBindings.kt").path
        assertTrue(
            "generated source not staged at $staged:\n$text",
            text.contains("""<src file="$staged" generated="true" />"""),
        )
        assertTrue("staged generated source missing", File(staged).exists())

        val mergedPath = File(fixture.workdir, "__app_app-merged-manifest.xml").path
        val merged = File(mergedPath).readText(Charsets.UTF_8)
        for (want in listOf(
            "package=\"dev.example\"",
            "xmlns:ns0=\"http://schemas.android.com/apk/res/android\"",
            "ns0:versionCode=\"1\"",
            "ns0:versionName=\"0.1.0\"",
            "ns0:minSdkVersion=\"26\"",
            "ns0:targetSdkVersion=\"36\"",
        )) {
            assertTrue("merged manifest does not contain \"$want\":\n$merged", merged.contains(want))
        }

        val argText = File(fixture.argsFile).readText(Charsets.UTF_8)
        for (want in listOf(
            "--project\n$projectPath",
            "--compile-sdk-version\n36",
            "--sdk-home\n${fixture.sdkHome}",
            "--config\n${fixture.configPath}",
            "--exitcode\n-Werror",
            "--text\nstdout",
            "--quiet",
        )) {
            assertTrue("lint args do not contain \"$want\":\n$argText", argText.contains(want))
        }
        assertTrue(
            "ANDROID_USER_HOME not set: ${result.stdout}",
            result.stdout.contains("user_home=${File(fixture.workdir, "android-user").path}"),
        )
        assertTrue(
            "LINT_OPTS not set: ${result.stdout}",
            result.stdout.contains("lint_opts=-Duser.home=${fixture.workdir}"),
        )
    }

    @Test
    fun exitCodeIsPropagated() {
        val fixture = newLintFixture()
        fixture.environment.set("ANDROID_LINT", fixture.lintPath)
        fixture.environment.set("LINT_EXIT_CODE", "3")

        val result = runLint(fixture.environment, fixture.recordsPath, fixture.configPath)
        assertEquals("exit ${result.code}", 3, result.code)
    }

    @Test
    fun lintModulesEnvironmentFallback() {
        val fixture = newLintFixture()
        fixture.environment.set("ANDROID_LINT", fixture.lintPath)
        fixture.environment.set("LINT_MODULES", fixture.recordsPath)

        val result = runLint(fixture.environment)
        assertEquals("exit ${result.code}: ${result.stderr}", 0, result.code)
    }

    @Test
    fun missingRunfilesFails() {
        val environment = MapEnvironment()
            .set("TEST_SRCDIR", "")
            .set("TEST_WORKSPACE", "")
        val result = runLint(environment, "modules.jsonl")
        assertTrue(
            "exit ${result.code}: ${result.stderr}",
            result.code == 1 && result.stderr.contains("TEST_SRCDIR/TEST_WORKSPACE are not set"),
        )
    }

    @Test
    fun missingArgumentsFails() {
        val temp = Files.createTempDirectory("lint-runner-").toFile()
        val environment = MapEnvironment()
            .set("TEST_SRCDIR", temp.path)
            .set("TEST_WORKSPACE", "ws")
            .set("LINT_MODULES", "")
        val result = runLint(environment)
        assertTrue(
            "exit ${result.code}: ${result.stderr}",
            result.code == 1 && result.stderr.contains("usage: lint_runner"),
        )
    }

    @Test
    fun findLintUsesAndroidHome() {
        val temp = Files.createTempDirectory("lint-sdk-").toFile()
        val lintPath = File(temp, "cmdline-tools/23.0/bin/lint")
        lintPath.parentFile.mkdirs()
        lintPath.writeText("#!/bin/sh\n", Charsets.UTF_8)
        lintPath.setExecutable(true)
        val environment = MapEnvironment()
            .set("ANDROID_LINT", "")
            .set("ANDROID_HOME", temp.path)
            .set("ANDROID_SDK_ROOT", "")

        assertEquals(lintPath.path, findLint(environment))
    }

    @Test
    fun findLintFailsWithoutSdk() {
        val environment = MapEnvironment()
            .set("ANDROID_LINT", "")
            .set("ANDROID_HOME", "")
            .set("ANDROID_SDK_ROOT", "")
        val error = try {
            findLint(environment)
            null
        } catch (exception: Exception) {
            exception
        }
        assertTrue(
            "unexpected error: $error",
            error != null && error.message!!.contains("Android Lint was not found"),
        )
    }

    @Test
    fun stagedGeneratedSourceWithoutPackageKeepsPath() {
        val temp = Files.createTempDirectory("lint-staged-").toFile()
        val path = File(temp, "NoPackage.kt")
        writeFile(path, "class X\n")
        val staged = stagedGeneratedSource(
            LintModule(
                label = "//m:m",
                name = "//m:m",
                packageName = "",
                android = false,
                app = false,
                srcs = emptyList(),
                generatedSrcs = emptyList(),
                resources = emptyList(),
                manifest = null,
                deps = emptyList(),
                jars = emptyList(),
            ),
            path.path,
            temp.path,
        )
        assertEquals(path.path, staged)
    }
}
