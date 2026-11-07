package dev.proflyder.home.tools.apkruntimeclosure

import java.io.ByteArrayOutputStream
import java.io.File
import java.nio.file.Files
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

private fun putU4(data: ByteArray, offset: Int, value: Int) {
    data[offset] = (value and 0xff).toByte()
    data[offset + 1] = ((value ushr 8) and 0xff).toByte()
    data[offset + 2] = ((value ushr 16) and 0xff).toByte()
    data[offset + 3] = ((value ushr 24) and 0xff).toByte()
}

/** Creates the smallest DEX that defines the given descriptors. */
private fun buildDex(descriptors: List<String>): ByteArray {
    val count = descriptors.size
    val stringIdsOff = 0x70
    val typeIdsOff = stringIdsOff + 4 * count
    val classDefsOff = typeIdsOff + 4 * count
    val dataOff = classDefsOff + 32 * count
    val encoded = descriptors.map { it.toByteArray(Charsets.UTF_8) }
    val data = ByteArray(dataOff + encoded.sumOf { 1 + it.size + 1 })
    val magic = byteArrayOf('d'.code.toByte(), 'e'.code.toByte(), 'x'.code.toByte(), '\n'.code.toByte())
    magic.copyInto(data, 0)
    putU4(data, 0x38, count)
    putU4(data, 0x3c, stringIdsOff)
    putU4(data, 0x40, count)
    putU4(data, 0x44, typeIdsOff)
    putU4(data, 0x60, count)
    putU4(data, 0x64, classDefsOff)

    var cursor = dataOff
    for ((index, bytes) in encoded.withIndex()) {
        putU4(data, stringIdsOff + 4 * index, cursor)
        // UTF-16 length is skipped by the parser; ASCII descriptors fit in one
        // uleb128 byte.
        data[cursor] = bytes.size.toByte()
        cursor += 1
        bytes.copyInto(data, cursor)
        cursor += bytes.size
        data[cursor] = 0
        cursor += 1
        putU4(data, typeIdsOff + 4 * index, index)
        putU4(data, classDefsOff + 32 * index, index)
    }
    return data
}

private fun writeZip(path: File, files: Map<String, ByteArray>) {
    ZipOutputStream(path.outputStream()).use { writer ->
        for ((name, payload) in files) {
            writer.putNextEntry(ZipEntry(name))
            writer.write(payload)
            writer.closeEntry()
        }
    }
}

/**
 * Writes a class file with a constant pool that names the class and every
 * reference. The parser only reads the constant pool.
 */
private fun buildClass(name: String, refs: List<String>): ByteArray {
    val pool = ByteArrayOutputStream()
    var count = 1
    fun add(text: String): Int {
        val index = count
        count++
        pool.write(1)
        pool.write((text.length shr 8) and 0xff)
        pool.write(text.length and 0xff)
        pool.write(text.toByteArray(Charsets.UTF_8))
        return index
    }
    val nameIndex = add(name)
    pool.write(7)
    pool.write((nameIndex shr 8) and 0xff)
    pool.write(nameIndex and 0xff)
    count++
    for (ref in refs) {
        val refIndex = add(ref)
        pool.write(7)
        pool.write((refIndex shr 8) and 0xff)
        pool.write(refIndex and 0xff)
        count++
    }

    val out = ByteArrayOutputStream()
    out.write(byteArrayOf(0xCA.toByte(), 0xFE.toByte(), 0xBA.toByte(), 0xBE.toByte(), 0, 0, 0, 52))
    out.write((count shr 8) and 0xff)
    out.write(count and 0xff)
    out.write(pool.toByteArray())
    return out.toByteArray()
}

private class ClosureResult(val code: Int, val stdout: String, val stderr: String)

private fun runClosure(vararg args: String): ClosureResult {
    val stdout = StringBuilder()
    val stderr = StringBuilder()
    val code = run(arrayOf(*args), stdout, stderr)
    return ClosureResult(code, stdout.toString(), stderr.toString())
}

private class Fixture {
    val temp: File = Files.createTempDirectory("apk-closure-test-").toFile()
    val apk: File = File(temp, "app.apk")
    val jar: File = File(temp, "app_deploy.jar")
}

class ApkRuntimeClosureTest {
    @Test
    fun mainSuccess() {
        val fixture = Fixture()
        writeZip(fixture.apk, mapOf("classes.dex" to buildDex(REQUIRED_DEX_CLASSES)))
        writeZip(fixture.jar, mapOf("com/example/Foo.class" to buildClass("com/example/Foo", emptyList())))

        val result = runClosure(fixture.apk.path, fixture.jar.path)
        assertEquals("exit ${result.code}: ${result.stdout}${result.stderr}", 0, result.code)
        assertTrue("missing success message: ${result.stdout}", result.stdout.contains("APK runtime closure OK"))
    }

    @Test
    fun missingRequiredDexClassFails() {
        val fixture = Fixture()
        writeZip(fixture.apk, mapOf("classes.dex" to buildDex(REQUIRED_DEX_CLASSES.take(2))))
        writeZip(fixture.jar, mapOf("com/example/Foo.class" to buildClass("com/example/Foo", emptyList())))

        val result = runClosure(fixture.apk.path, fixture.jar.path)
        assertTrue("missing DEX class accepted", result.code != 0)
        for (descriptor in REQUIRED_DEX_CLASSES.drop(2)) {
            assertTrue(
                "failure does not name $descriptor: ${result.stderr}",
                result.stderr.contains(descriptor),
            )
        }
    }

    @Test
    fun dexWithoutClassesFails() {
        val fixture = Fixture()
        writeZip(fixture.apk, mapOf("resources.arsc" to "x".toByteArray(Charsets.UTF_8)))
        writeZip(fixture.jar, mapOf("com/example/Foo.class" to buildClass("com/example/Foo", emptyList())))

        val result = runClosure(fixture.apk.path, fixture.jar.path)
        assertTrue(
            "exit ${result.code}: ${result.stderr}",
            result.code != 0 && result.stderr.contains("no classes*.dex entries found"),
        )
    }

    @Test
    fun unparseableDexFails() {
        val fixture = Fixture()
        writeZip(fixture.apk, mapOf("classes.dex" to "not a dex".toByteArray(Charsets.UTF_8)))
        writeZip(fixture.jar, mapOf("com/example/Foo.class" to buildClass("com/example/Foo", emptyList())))

        val result = runClosure(fixture.apk.path, fixture.jar.path)
        assertTrue(
            "exit ${result.code}: ${result.stderr}",
            result.code != 0 &&
                result.stderr.contains("cannot parse DEX class_defs: not a DEX file"),
        )
    }

    @Test
    fun selfTestRejectsReferencedButUndefinedClass() {
        val fixture = Fixture()
        writeZip(fixture.apk, mapOf("classes.dex" to buildDex(REQUIRED_DEX_CLASSES)))

        val result = runClosure("--self-test", fixture.apk.path)
        assertEquals("exit ${result.code}: ${result.stdout}${result.stderr}", 0, result.code)
        assertTrue(
            "missing self-test message: ${result.stdout}",
            result.stdout.contains("DEX definition self-test OK"),
        )
    }

    @Test
    fun selfTestWithoutRequiredClassFails() {
        val fixture = Fixture()
        writeZip(fixture.apk, mapOf("classes.dex" to buildDex(REQUIRED_DEX_CLASSES.drop(1))))

        val result = runClosure("--self-test", fixture.apk.path)
        assertTrue(
            "exit ${result.code}: ${result.stderr}",
            result.code != 0 && result.stderr.contains("required class"),
        )
    }

    @Test
    fun deployJarMissingReferenceFails() {
        val fixture = Fixture()
        writeZip(fixture.apk, mapOf("classes.dex" to buildDex(REQUIRED_DEX_CLASSES)))
        writeZip(
            fixture.jar,
            mapOf(
                "com/example/Foo.class" to buildClass("com/example/Foo", listOf("com/example/Bar")),
            ),
        )

        val result = runClosure(fixture.apk.path, fixture.jar.path)
        assertTrue(
            "exit ${result.code}: ${result.stderr}",
            result.code != 0 && result.stderr.contains("runtime closure is missing com.example.Bar"),
        )
    }

    @Test
    fun platformAndOptionalReferencesAreAllowed() {
        val fixture = Fixture()
        writeZip(fixture.apk, mapOf("classes.dex" to buildDex(REQUIRED_DEX_CLASSES)))
        writeZip(
            fixture.jar,
            mapOf(
                "com/example/Foo.class" to buildClass(
                    "com/example/Foo",
                    listOf(
                        "java/lang/String",
                        "org/bouncycastle/Foo",
                        "com/google/firebase/components/Component\$ComponentType",
                    ),
                ),
            ),
        )

        val result = runClosure(fixture.apk.path, fixture.jar.path)
        assertEquals("exit ${result.code}: ${result.stdout}${result.stderr}", 0, result.code)
    }

    @Test
    fun unparseableClassFails() {
        val fixture = Fixture()
        writeZip(fixture.apk, mapOf("classes.dex" to buildDex(REQUIRED_DEX_CLASSES)))
        val corrupt = byteArrayOf(
            0xCA.toByte(), 0xFE.toByte(), 0xBA.toByte(), 0xBE.toByte(),
            0, 0, 0, 52, 0, 2, 99,
        )
        writeZip(fixture.jar, mapOf("com/example/Bad.class" to corrupt))

        val result = runClosure(fixture.apk.path, fixture.jar.path)
        assertTrue(
            "exit ${result.code}: ${result.stderr}",
            result.code != 0 && result.stderr.contains("cannot parse class in"),
        )
    }

    @Test
    fun usageExitsTwo() {
        val result = runClosure("only-one-argument")
        assertTrue(
            "exit ${result.code}: ${result.stderr}",
            result.code == 2 && result.stderr.contains("usage:"),
        )
    }
}
