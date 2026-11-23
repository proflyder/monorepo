package dev.proflyder.home.tools.lintrunner

import java.io.File
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

private fun writeFile(path: File, content: String) {
    path.parentFile.mkdirs()
    path.writeText(content, Charsets.UTF_8)
}

private const val SPEC = """
{
  "mode": "MODE",
  "sdk": "sdk",
  "config": "lint.xml",
  "module": {
    "label": "//app:app_lib", "package": "dev.example", "android": true, "app": true,
    "srcs": ["app/src/main/kotlin/dev/example/Main.kt", "out/gen/ViewBindings.kt"],
    "generated_srcs": ["out/gen/ViewBindings.kt"],
    "resources": ["app/src/main/res/layout/a.xml"],
    "manifest": "app/src/main/AndroidManifest.xml",
    "jars": ["out/app.jar"],
    "partial": "out/partial/app",
    "deps": ["//core/ui:ui"]
  },
  "libraries": [
    {
      "label": "//core/ui:ui", "package": "dev.example.ui", "android": true, "app": false,
      "srcs": [], "generated_srcs": [],
      "resources": ["core/ui/src/main/res/values/strings.xml"],
      "manifest": "core/ui/src/main/AndroidManifest.xml",
      "jars": ["out/ui.jar"],
      "partial": "out/partial/ui",
      "deps": []
    }
  ],
  "external_jars": [
    "external/maven/v1/https/repo1.maven.org/maven2/androidx/core/core/1.17.0/processed_core.jar",
    "external/other/lib.jar"
  ],
  "coordinates": {
    "external/maven/v1/https/repo1.maven.org/maven2/androidx/core/core/1.17.0/processed_core.jar": "androidx.core:core:1.17.0"
  }
}
"""

class LintRunnerTest {
    private fun fixture(): File {
        val root = Files.createTempDirectory("lint-runner-").toFile()
        writeFile(File(root, "app/src/main/kotlin/dev/example/Main.kt"), "package dev.example\n")
        writeFile(File(root, "out/gen/ViewBindings.kt"), "package dev.example.databinding\nclass X\n")
        writeFile(File(root, "app/src/main/res/layout/a.xml"), "<LinearLayout />\n")
        writeFile(File(root, "app/src/main/AndroidManifest.xml"), "<manifest package=\"dev.example\">\n<uses-sdk />\n</manifest>\n")
        writeFile(File(root, "core/ui/src/main/res/values/strings.xml"), "<resources />\n")
        writeFile(File(root, "core/ui/src/main/AndroidManifest.xml"), "<manifest />\n")
        writeFile(File(root, "out/app.jar"), "")
        writeFile(File(root, "out/ui.jar"), "")
        writeFile(
            File(root, "external/maven/v1/https/repo1.maven.org/maven2/androidx/core/core/1.17.0/processed_core.jar"),
            "",
        )
        writeFile(File(root, "external/other/lib.jar"), "")
        return root
    }

    private fun models(mode: String): Pair<List<File>, File> {
        val root = fixture()
        val work = File(root, "work")
        val spec = readSpec(SPEC.replace("MODE", mode))
        return ModelWriter(root, work).write(spec) to root
    }

    @Test
    fun sourceRootStripsPackageDirectories() {
        assertEquals("a/src/main/kotlin", sourceRoot("a/src/main/kotlin/dev/x/A.kt", "package dev.x\n"))
        assertEquals("a/flat", sourceRoot("a/flat/A.kt", "package dev.x\n"))
        assertEquals("a/flat", sourceRoot("a/flat/A.kt", "class A\n"))
    }

    @Test
    fun minimalRootsDropsNestedDirectories() {
        assertEquals(listOf("a/src", "b"), minimalRoots(listOf("a/src/x", "b", "a/src", "a/src")))
    }

    @Test
    fun jarCoordinatesComeFromMavenTags() {
        val coordinates = mapOf("a.jar" to "androidx.core:core:1.17.0")
        assertEquals("androidx.core:core:1.17.0", jarCoordinate("a.jar", coordinates))
        assertTrue(jarCoordinate("external/other/lib.jar", coordinates).startsWith("bazel.jar:"))
        assertEquals(":projects:app:app_lib", modulePath("//projects/app:app_lib"))
    }

    @Test
    fun analyzeModelOmitsDependencyPartialResults() {
        val (models, root) = models("analyze")
        assertEquals(1, models.size)
        val model = models.single()
        val module = File(model, "module.xml").readText()
        assertTrue(module.contains("type=\"APP\""))
        assertTrue(module.contains("checkDependencies=\"true\""))
        val variant = File(model, "main.xml").readText()
        assertTrue(variant.contains("javaDirectories=\"${File(root, "app/src/main/kotlin").path}\""))
        assertTrue(variant.contains("resDirectories=\"${File(root, "app/src/main/res").path}\""))
        assertTrue(variant.contains("partialResultsDir=\"${File(root, "out/partial/app").path}\""))
        assertTrue(variant.contains("generatedSourceFolders="))
        assertFalse(variant.contains("minSdkVersion"))
        assertTrue(File(root, "work/manifests").listFiles()!!.any { it.readText().contains("android:versionCode=\"1\"") })
        val libraries = File(model, "main-artifact-libraries.xml").readText()
        assertTrue(libraries.contains("resFolder=\"${File(root, "core/ui/src/main/res").path}\""))
        assertTrue(libraries.contains("name=\"androidx.core:core:1.17.0\""))
        assertFalse(libraries.contains("partialResultsDir"))
        val dependencies = File(model, "main-artifact-dependencies.xml").readText()
        assertTrue(dependencies.contains("<compile roots="))
        assertTrue(dependencies.contains("<package roots="))
    }

    @Test
    fun reportModelPointsLibrariesAtPartialResults() {
        val (models, root) = models("report")
        assertEquals(2, models.size)
        val libraries = File(models[0], "main-artifact-libraries.xml").readText()
        assertTrue(libraries.contains("project=\":core:ui:ui\""))
        assertTrue(libraries.contains("androidx.core:core:1.17.0"))
        val library = File(models[1], "main.xml").readText()
        assertTrue(library.contains("partialResultsDir=\"${File(root, "out/partial/ui").path}\""))
        assertTrue(library.contains("minSdkVersion=\"26\""))
    }

    @Test
    fun reportCommandFailsOnErrors() {
        val spec = readSpec(SPEC.replace("MODE", "report"))
        val command = lintCommand(spec, File("/r"), listOf(File("/m1"), File("/m2")), File("/w"), "/jdk")
        assertEquals("/jdk/bin/java", command.first())
        assertTrue(command.containsAll(listOf("--report-only", "-Werror", "--config", "/r/lint.xml", "/m1", "/m2")))
        assertTrue(command.contains("ROOT=/r"))
    }

    @Test
    fun warningsAndErrorsAreCountedFromXmlReport() {
        val xml = "<issues>\n<issue id=\"A\" severity=\"Error\" />\n<issue id=\"B\" severity=\"Warning\" />\n" +
            "<issue id=\"C\" severity=\"Fatal\" />\n<issue id=\"D\" severity=\"Information\" />\n</issues>"
        assertEquals(3, countFindings(xml))
        assertEquals(0, countFindings("<issues />"))
    }

    @Test
    fun appManifestGetsVersion() {
        val text = appManifest("<manifest xmlns:android=\"x\" package=\"p\">\n</manifest>")
        assertTrue(text.startsWith("<manifest android:versionCode=\"1\" android:versionName=\"0.1.0\" xmlns"))
    }

    @Test
    fun missingSdkIsReported() {
        val root = fixture()
        val spec = File(root, "spec.json")
        spec.writeText(SPEC.replace("MODE", "analyze"))
        val err = StringBuilder()
        assertEquals(1, run(arrayOf(spec.path), StringBuilder(), err))
        assertTrue(err.contains("cmdline-tools;23.0"))
    }
}
