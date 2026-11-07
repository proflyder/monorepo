// Command apk_runtime_closure is the APK-level runtime closure gate for the
// Android client.
//
// This test inspects build artifacts rather than the JVM test classpath, because
// the Robolectric classpath contains Guava (as a test dependency) and therefore
// cannot detect a missing runtime ListenableFuture in the APK.
//
// Two independent checks run:
//
//  1. DEX definition check: the packaged classes*.dex must *define* the classes
//     that androidx.profileinstaller / androidx.concurrent.futures need at
//     runtime. Parsing DEX class_defs proves a definition; searching the string
//     table does not, because a missing class is still referenced by name there.
//  2. Closure check: every class referenced from the deploy jar (the exact set of
//     classes D8 receives) must be defined in that jar or be a platform/optional
//     class. This catches *any* missing transitive runtime dependency, not only
//     the one that caused the ProfileInstaller crash.
//
// The --self-test mode mutates a copy of the real DEX so that a required
// descriptor remains referenced by the string table but loses its class_def.
// The definition check must reject it. This runs as part of the Bazel test so
// the gate cannot silently degrade into a reference-only check.
package dev.proflyder.home.tools.apkruntimeclosure

import java.io.File
import java.nio.file.Files
import java.nio.file.Paths
import java.util.zip.ZipEntry
import java.util.zip.ZipFile
import java.util.zip.ZipOutputStream

val REQUIRED_DEX_CLASSES = listOf(
    "Lcom/google/common/util/concurrent/ListenableFuture;",
    "Landroidx/concurrent/futures/AbstractResolvableFuture;",
    "Landroidx/concurrent/futures/CallbackToFutureAdapter;",
    "Landroidx/profileinstaller/ProfileInstaller;",
)

private val PLATFORM_PREFIXES = listOf(
    "java.",
    "javax.",
    "jdk.",
    "sun.",
    "com.sun.",
    "android.",
    "dalvik.",
    "org.xml.",
    "org.xmlpull.",
    "org.w3c.",
    "org.json.",
    "org.apache.",
    "libcore.",
)

private val OPTIONAL_PREFIXES = listOf(
    "com.android.tools.r8.annotations.",
    "com.google.auto.value.",
    "com.google.errorprone.annotations.",
    "com.google.j2objc.annotations.",
    "org.checkerframework.",
    "org.bouncycastle.",
    "org.conscrypt.",
    "org.openjsse.",
)

private val OPTIONAL_CLASSES = setOf(
    "com.google.firebase.components.Component\$ComponentType",
    "com.google.firebase.components.Component\$Instantiation",
)

private val DEX_NAME_PATTERN = Regex("^classes\\d*\\.dex$")
private val DESCRIPTOR_CLASS_PATTERN = Regex("L([A-Za-z0-9_$/]+);")

private class ToolError(message: String) : Exception(message)

private data class DexEntry(val name: String, val data: ByteArray)

private fun readU4(data: ByteArray, offset: Int): Int {
    if (offset < 0 || offset + 4 > data.size) {
        return 0
    }
    return (data[offset].toInt() and 0xff) or
        ((data[offset + 1].toInt() and 0xff) shl 8) or
        ((data[offset + 2].toInt() and 0xff) shl 16) or
        ((data[offset + 3].toInt() and 0xff) shl 24)
}

private fun readU2Be(data: ByteArray, offset: Int): Int {
    if (offset < 0 || offset + 2 > data.size) {
        return 0
    }
    return ((data[offset].toInt() and 0xff) shl 8) or (data[offset + 1].toInt() and 0xff)
}

private fun putU4Le(data: ByteArray, offset: Int, value: Int) {
    data[offset] = (value and 0xff).toByte()
    data[offset + 1] = ((value ushr 8) and 0xff).toByte()
    data[offset + 2] = ((value ushr 16) and 0xff).toByte()
    data[offset + 3] = ((value ushr 24) and 0xff).toByte()
}

private fun readUleb128(data: ByteArray, start: Int): Pair<Int, Int> {
    var result = 0
    var shift = 0
    var offset = start
    while (true) {
        if (offset >= data.size) {
            throw ToolError("unexpected end of DEX")
        }
        val byte = data[offset].toInt() and 0xff
        offset += 1
        result = result or ((byte and 0x7f) shl shift)
        if (byte and 0x80 == 0) {
            return result to offset
        }
        shift += 7
    }
}

private fun decodeUtf8(raw: ByteArray): String = String(raw, Charsets.UTF_8)

private fun dexStringAt(data: ByteArray, stringIdsSize: Int, stringIdsOff: Int, stringId: Int): String {
    if (stringId >= stringIdsSize) {
        throw ToolError("string id out of range")
    }
    val dataOff = readU4(data, stringIdsOff + 4 * stringId)
    val cursor = readUleb128(data, dataOff).second
    var end = -1
    var index = cursor
    while (index < data.size) {
        if (data[index] == 0.toByte()) {
            end = index
            break
        }
        index += 1
    }
    if (end < 0) {
        throw ToolError("subsection not found")
    }
    return decodeUtf8(data.copyOfRange(cursor, end))
}

private fun dexDefinedTypes(data: ByteArray): Set<String> {
    if (data.size < 4 || data[0] != 'd'.code.toByte() || data[1] != 'e'.code.toByte() ||
        data[2] != 'x'.code.toByte() || data[3] != '\n'.code.toByte()
    ) {
        throw ToolError("not a DEX file")
    }
    val stringIdsSize = readU4(data, 0x38)
    val stringIdsOff = readU4(data, 0x3c)
    val typeIdsSize = readU4(data, 0x40)
    val typeIdsOff = readU4(data, 0x44)
    val classDefsSize = readU4(data, 0x60)
    val classDefsOff = readU4(data, 0x64)
    if (typeIdsOff + 4 * typeIdsSize > data.size || classDefsOff + 32 * classDefsSize > data.size) {
        throw ToolError("DEX section offsets are out of bounds")
    }

    fun descriptor(typeId: Int): String {
        if (typeId >= typeIdsSize) {
            throw ToolError("type id out of range")
        }
        return dexStringAt(data, stringIdsSize, stringIdsOff, readU4(data, typeIdsOff + 4 * typeId))
    }

    val defined = mutableSetOf<String>()
    for (index in 0 until classDefsSize) {
        defined.add(descriptor(readU4(data, classDefsOff + 32 * index)))
    }
    return defined
}

/**
 * Maps each defined descriptor to the offset of its class_def entry, preserving
 * class_def order.
 */
private fun dexClassDefOffsets(data: ByteArray): Pair<Map<String, Int>, List<String>> {
    val stringIdsOff = readU4(data, 0x3c)
    val stringIdsSize = readU4(data, 0x38)
    val typeIdsOff = readU4(data, 0x44)
    val classDefsSize = readU4(data, 0x60)
    val classDefsOff = readU4(data, 0x64)

    val result = mutableMapOf<String, Int>()
    val order = mutableListOf<String>()
    for (index in 0 until classDefsSize) {
        val entry = classDefsOff + 32 * index
        val classIdx = readU4(data, entry)
        val descriptorIdx = readU4(data, typeIdsOff + 4 * classIdx)
        val name = try {
            dexStringAt(data, stringIdsSize, stringIdsOff, descriptorIdx)
        } catch (_: ToolError) {
            continue
        }
        if (!result.containsKey(name)) {
            result[name] = entry
            order.add(name)
        }
    }
    return result to order
}

private fun readZipFile(file: ZipFile, entry: ZipEntry): ByteArray =
    file.getInputStream(entry).use { it.readBytes() }

private fun apkDexBytes(apk: String): List<DexEntry> {
    ZipFile(File(apk)).use { archive ->
        val byName = mutableMapOf<String, ZipEntry>()
        val names = mutableListOf<String>()
        val entries = archive.entries()
        while (entries.hasMoreElements()) {
            val entry = entries.nextElement()
            if (DEX_NAME_PATTERN.matches(entry.name)) {
                names.add(entry.name)
                byName[entry.name] = entry
            }
        }
        names.sort()
        return names.map { name -> DexEntry(name, readZipFile(archive, byName.getValue(name))) }
    }
}

internal fun checkDex(apk: String): List<String> {
    val dexes = apkDexBytes(apk)
    if (dexes.isEmpty()) {
        return listOf("$apk: no classes*.dex entries found")
    }
    val defined = mutableSetOf<String>()
    for (dex in dexes) {
        val types = try {
            dexDefinedTypes(dex.data)
        } catch (error: ToolError) {
            return listOf("$apk:${dex.name}: cannot parse DEX class_defs: ${error.message}")
        }
        defined.addAll(types)
    }
    val failures = mutableListOf<String>()
    for (descriptor in REQUIRED_DEX_CLASSES) {
        if (!defined.contains(descriptor)) {
            failures.add("DEX in $apk does not define $descriptor")
        }
    }
    return failures
}

internal fun selfTest(apk: String): List<String> {
    val failures = mutableListOf<String>()
    val dexes = apkDexBytes(apk)
    if (dexes.isEmpty()) {
        return listOf("$apk: no classes*.dex entries found for self-test")
    }
    val descriptor = REQUIRED_DEX_CLASSES[0]
    for (dex in dexes) {
        val (offsets, order) = dexClassDefOffsets(dex.data)
        val entry = offsets[descriptor] ?: continue
        var target = ""
        for (candidate in order) {
            if (candidate != descriptor) {
                target = candidate
                break
            }
        }
        if (target.isEmpty()) {
            return listOf("$apk:${dex.name}: self-test needs a second defined class")
        }
        val typeIdsOff = readU4(dex.data, 0x44)
        val stringIdsOff = readU4(dex.data, 0x3c)
        val stringIdsSize = readU4(dex.data, 0x38)
        val typeIdsSize = readU4(dex.data, 0x40)

        var targetTypeIdx = -1
        for (typeIdx in 0 until typeIdsSize) {
            val stringId = readU4(dex.data, typeIdsOff + 4 * typeIdx)
            val name = dexStringAt(dex.data, stringIdsSize, stringIdsOff, stringId)
            if (name == target) {
                targetTypeIdx = typeIdx
                break
            }
        }
        if (targetTypeIdx < 0) {
            return listOf("$apk:${dex.name}: self-test could not find target type")
        }
        val mutated = dex.data.copyOf()
        putU4Le(mutated, entry, targetTypeIdx)
        if (indexOf(mutated, descriptor.toByteArray(Charsets.UTF_8)) < 0) {
            failures.add("self-test setup: $descriptor is no longer referenced in ${dex.name}")
            continue
        }
        val stillDefined = try {
            dexDefinedTypes(mutated)
        } catch (error: ToolError) {
            failures.add("self-test could not parse mutated DEX: ${error.message}")
            continue
        }
        if (stillDefined.contains(descriptor)) {
            failures.add("self-test: mutated DEX still defines $descriptor")
            continue
        }

        val mutatedApk = writeMutatedApk(apk, dex.name, mutated)
        val gateFailures = checkDex(mutatedApk)
        File(mutatedApk).parentFile?.deleteRecursively()
        var found = false
        for (failure in gateFailures) {
            if (failure.contains(descriptor) && failure.contains("does not define")) {
                found = true
                break
            }
        }
        if (!found) {
            failures.add(
                "self-test: check_dex accepted referenced-but-undefined $descriptor: " +
                    gateFailures.joinToString(", "),
            )
        }
        return failures
    }
    failures.add("self-test: required class $descriptor not found in $apk")
    return failures
}

private fun indexOf(haystack: ByteArray, needle: ByteArray): Int {
    if (needle.isEmpty()) {
        return 0
    }
    for (index in 0..haystack.size - needle.size) {
        var match = true
        for (inner in needle.indices) {
            if (haystack[index + inner] != needle[inner]) {
                match = false
                break
            }
        }
        if (match) {
            return index
        }
    }
    return -1
}

private fun writeMutatedApk(source: String, dexName: String, payload: ByteArray): String {
    val tempDir = Files.createTempDirectory("apk-closure-")
    val target = tempDir.resolve("mutated.apk")

    ZipFile(File(source)).use { archive ->
        ZipOutputStream(Files.newOutputStream(target)).use { writer ->
            val entries = archive.entries()
            while (entries.hasMoreElements()) {
                val entry = entries.nextElement()
                val contents = if (entry.name == dexName) payload else readZipFile(archive, entry)
                val copy = ZipEntry(entry.name)
                copy.time = entry.time
                writer.putNextEntry(copy)
                writer.write(contents)
                writer.closeEntry()
            }
        }
    }
    return target.toString()
}

private class ConstantPool(val classes: Set<String>, val descriptors: Set<String>)

private fun constantPoolClasses(data: ByteArray): ConstantPool {
    val classes = mutableSetOf<String>()
    val descriptors = mutableSetOf<String>()
    if (data.size < 4 || data[0] != 0xCA.toByte() || data[1] != 0xFE.toByte() ||
        data[2] != 0xBA.toByte() || data[3] != 0xBE.toByte()
    ) {
        return ConstantPool(classes, descriptors)
    }
    val count = readU2Be(data, 8)
    var offset = 10
    val utf8 = mutableMapOf<Int, String>()
    val classNameIndices = mutableListOf<Int>()
    val descriptorIndices = mutableListOf<Int>()
    var index = 1
    while (index < count) {
        if (offset >= data.size) {
            throw ToolError("unexpected end of class file")
        }
        val tag = data[offset].toInt() and 0xff
        offset += 1
        when (tag) {
            1 -> { // CONSTANT_Utf8
                val length = readU2Be(data, offset)
                offset += 2
                var end = offset + length
                if (end > data.size) {
                    end = data.size
                }
                val raw = data.copyOfRange(minOf(offset, data.size), end)
                offset += length
                utf8[index] = decodeUtf8(raw)
            }
            3, 4 -> offset += 4 // Integer, Float
            5, 6 -> { // Long, Double take two slots
                offset += 8
                index += 1
            }
            7 -> { // Class
                classNameIndices.add(readU2Be(data, offset))
                offset += 2
            }
            8 -> offset += 2 // String
            9, 10, 11 -> offset += 4 // Fieldref, Methodref, InterfaceMethodref
            12 -> { // NameAndType
                offset += 2
                descriptorIndices.add(readU2Be(data, offset))
                offset += 2
            }
            15 -> offset += 3 // MethodHandle
            16 -> offset += 2 // MethodType
            17, 18 -> offset += 4 // Dynamic, InvokeDynamic
            19, 20 -> offset += 2 // Module, Package
            else -> throw ToolError("unknown constant pool tag $tag at $offset")
        }
        index += 1
    }
    for (classNameIndex in classNameIndices) {
        utf8[classNameIndex]?.let { classes.add(it) }
    }
    for (descriptorIndex in descriptorIndices) {
        utf8[descriptorIndex]?.let { descriptors.add(it) }
    }
    return ConstantPool(classes, descriptors)
}

internal fun normalise(name: String): String {
    var result = name.replace("/", ".")
    if (result.startsWith("[")) {
        result = result.trimStart('[')
        if (!result.startsWith("L")) {
            return ""
        }
        result = if (result.endsWith(";")) {
            result.substring(1, result.length - 1)
        } else {
            result.substring(1)
        }
    }
    if (result.startsWith("L") && result.endsWith(";")) {
        result = result.substring(1, result.length - 1)
    }
    if (result.length <= 1 || !result.contains(".")) {
        return ""
    }
    return result
}

private fun allowed(name: String): Boolean {
    if (OPTIONAL_CLASSES.contains(name)) {
        return true
    }
    for (prefix in PLATFORM_PREFIXES) {
        if (name.startsWith(prefix)) {
            return true
        }
    }
    for (prefix in OPTIONAL_PREFIXES) {
        if (name.startsWith(prefix)) {
            return true
        }
    }
    return false
}

internal fun checkDeployJar(deployJar: String): List<String> {
    val defined = mutableSetOf<String>()
    val payloads = mutableListOf<ByteArray>()
    ZipFile(File(deployJar)).use { archive ->
        val entries = archive.entries()
        while (entries.hasMoreElements()) {
            val entry = entries.nextElement()
            val name = entry.name
            if (!name.endsWith(".class") || name.startsWith("META-INF/")) {
                continue
            }
            defined.add(normalise(name.removeSuffix(".class")))
            payloads.add(readZipFile(archive, entry))
        }
    }

    val referenced = mutableSetOf<String>()
    for (payload in payloads) {
        val pool = try {
            constantPoolClasses(payload)
        } catch (error: ToolError) {
            return listOf("cannot parse class in $deployJar: ${error.message}")
        }
        for (name in pool.classes) {
            referenced.add(normalise(name))
        }
        for (descriptor in pool.descriptors) {
            for (match in DESCRIPTOR_CLASS_PATTERN.findAll(descriptor)) {
                referenced.add(normalise(match.groupValues[1]))
            }
        }
    }

    val missing = referenced
        .filter { it.isNotEmpty() && !allowed(it) && !defined.contains(it) }
        .sorted()
    return missing.map { "runtime closure is missing $it" }
}

private fun Appendable.line(text: String) {
    append(text)
    append('\n')
}

private fun cleanPath(path: String): String = Paths.get(path).normalize().toString()

fun run(args: Array<String>, stdout: Appendable, stderr: Appendable): Int {
    if (args.size == 2 && args[0] == "--self-test") {
        val apk = cleanPath(args[1])
        val failures = try {
            selfTest(apk)
        } catch (error: Exception) {
            stderr.line(error.message ?: error.toString())
            return 1
        }
        if (failures.isNotEmpty()) {
            stderr.line("DEX definition self-test failed:")
            for (failure in failures) {
                stderr.line("  - $failure")
            }
            return 1
        }
        stdout.line("DEX definition self-test OK: referenced-but-undefined class is rejected.")
        return 0
    }
    if (args.size != 2) {
        stderr.line("usage: apk_runtime_closure <app.apk> <app_deploy.jar>")
        return 2
    }
    val apk = cleanPath(args[0])
    val deployJar = cleanPath(args[1])
    val dexFails = try {
        checkDex(apk)
    } catch (error: Exception) {
        stderr.line(error.message ?: error.toString())
        return 1
    }
    val jarFails = try {
        checkDeployJar(deployJar)
    } catch (error: Exception) {
        stderr.line(error.message ?: error.toString())
        return 1
    }
    val failures = dexFails + jarFails
    if (failures.isNotEmpty()) {
        stderr.line("APK runtime closure check failed:")
        for (failure in failures) {
            stderr.line("  - $failure")
        }
        return 1
    }
    stdout.line(
        "APK runtime closure OK: required classes are defined in DEX, " +
            "no unresolved non-platform references in the deploy jar.",
    )
    return 0
}

fun main(args: Array<String>) {
    System.exit(run(args, System.out, System.err))
}
