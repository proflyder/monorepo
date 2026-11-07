// Command gen_view_binding generates Kotlin ViewBinding classes from Android
// layout XML files.
//
// rules_android has no ViewBinding support and the androidx.viewbinding
// artifact is not part of the pinned Maven closure, so Bazel modules use this
// generator (through `view_binding = True` in `metro_android_library`). The
// output mirrors the AGP shape: `<LayoutName>Binding` in `<package>.databinding`
// with typed `val`s per `android:id`, plus `inflate(...)` and `bind(...)`.
// Bindings implement `dev.proflyder.home.core.ui.ViewBinding` instead of
// `androidx.viewbinding.ViewBinding`.
//
// Usage: gen_view_binding --package PKG --output FILE LAYOUT.xml...
package dev.proflyder.home.tools.genviewbinding

import java.io.File
import javax.xml.stream.XMLInputFactory
import javax.xml.stream.XMLStreamConstants
import javax.xml.stream.XMLStreamReader

private const val ANDROID_NS = "http://schemas.android.com/apk/res/android"

private val VIEW_PACKAGE = setOf(
    "View",
    "ViewStub",
    "SurfaceView",
    "TextureView",
    "ViewGroup",
)

private val WEBKIT = setOf("WebView")

/** A view id and the tag that declares it. */
internal data class ViewRef(val name: String, val tag: String)

private fun Appendable.line(text: String) {
    append(text)
    append('\n')
}

// camel mirrors the Python splitting rule: [_\W]+ separates words, where the
// class is "underscore or not a word character".
internal fun camel(name: String, upperFirst: Boolean): String {
    val builder = StringBuilder()
    for (part in splitWords(name)) {
        builder.append(part[0].uppercaseChar())
        builder.append(part.substring(1))
    }
    val out = builder.toString()
    if (upperFirst) {
        return out
    }
    if (out.isEmpty()) {
        return out
    }
    return out[0].lowercaseChar() + out.substring(1)
}

internal fun splitWords(name: String): List<String> {
    val parts = mutableListOf<String>()
    val current = StringBuilder()
    fun flush() {
        if (current.isNotEmpty()) {
            parts.add(current.toString())
            current.clear()
        }
    }
    for (ch in name) {
        if (ch.isLetter() || ch.isDigit()) {
            current.append(ch)
        } else {
            flush()
        }
    }
    flush()
    return parts
}

internal fun qualified(tag: String): String {
    if (tag.contains(".")) {
        return tag
    }
    if (tag in VIEW_PACKAGE) {
        return "android.view.$tag"
    }
    if (tag in WEBKIT) {
        return "android.webkit.$tag"
    }
    return "android.widget.$tag"
}

internal fun tagName(namespace: String?, local: String): String {
    if (namespace.isNullOrEmpty()) {
        return local
    }
    return "{$namespace}$local"
}

/** Returns (rootTag, views) in document order, deduplicated by id name. */
internal fun collect(reader: XMLStreamReader): Pair<String, List<ViewRef>> {
    var rootTag = ""
    var haveRoot = false
    val seen = mutableSetOf<String>()
    val views = mutableListOf<ViewRef>()
    while (reader.hasNext()) {
        val event = reader.next()
        if (event != XMLStreamConstants.START_ELEMENT) {
            continue
        }
        val tag = tagName(reader.namespaceURI, reader.localName)
        if (!haveRoot) {
            rootTag = tag
            haveRoot = true
        }
        var raw: String? = null
        for (index in 0 until reader.attributeCount) {
            if (reader.getAttributeNamespace(index) == ANDROID_NS &&
                reader.getAttributeLocalName(index) == "id"
            ) {
                raw = reader.getAttributeValue(index)
                break
            }
        }
        if (raw == null ||
            !(raw.startsWith("@+id/") || raw.startsWith("@id/"))
        ) {
            continue
        }
        val parts = raw.split("/", limit = 2)
        if (parts.size != 2) {
            continue
        }
        val name = parts[1]
        if (seen.contains(name)) {
            continue
        }
        seen.add(name)
        val elementTag = if (tag == "include") "View" else tag
        views.add(ViewRef(name, elementTag))
    }
    if (!haveRoot) {
        throw IllegalArgumentException("no root element")
    }
    return rootTag to views
}

private fun parseError(message: String): Int {
    System.err.println(message)
    return 1
}

internal fun render(layoutPath: String): String {
    val layout = fileStem(layoutPath)
    val cls = camel(layout, true) + "Binding"

    val reader = XMLInputFactory.newInstance().createXMLStreamReader(File(layoutPath).inputStream())
    val rootTag: String
    val views: List<ViewRef>
    try {
        val collected = collect(reader)
        rootTag = collected.first
        views = collected.second
    } catch (error: Exception) {
        val detail = error.message ?: error.toString()
        throw IllegalArgumentException("$layoutPath: $detail")
    } finally {
        try {
            reader.close()
        } catch (_: Exception) {
            // Keep the primary failure; a failing close cannot change it.
        }
    }
    if (rootTag == "merge") {
        throw IllegalArgumentException("$layoutPath: <merge> roots are not supported")
    }
    val rootType = qualified(rootTag)

    val taken = mutableSetOf("root")
    val props = mutableListOf<Prop>()
    for (item in views) {
        var name = camel(item.name, false)
        while (taken.contains(name)) {
            name += "View"
        }
        taken.add(name)
        props.add(Prop(name, item.name, qualified(item.tag)))
    }

    val lines = mutableListOf<String>()
    lines.add("class $cls private constructor(")
    lines.add("    override val root: $rootType,")
    for (item in props) {
        lines.add("    val ${item.prop}: ${item.typ},")
    }
    lines.add(") : ViewBinding {")
    lines.add("    companion object {")
    lines.add("        fun inflate(")
    lines.add("            inflater: LayoutInflater,")
    lines.add("            parent: ViewGroup? = null,")
    lines.add("            attachToParent: Boolean = false,")
    lines.add("        ): $cls {")
    lines.add("            val root = inflater.inflate(R.layout.$layout, parent, false)")
    lines.add("            if (attachToParent && parent != null) parent.addView(root)")
    lines.add("            return bind(root)")
    lines.add("        }")
    lines.add("")
    lines.add("        fun bind(view: View): $cls = $cls(")
    lines.add("            view as $rootType,")
    for (item in props) {
        lines.add(
            "            view.findViewById<${item.typ}>(R.id.${item.name}) ?: " +
                "error(\"Missing view @id/${item.name} in $layout\"),",
        )
    }
    lines.add("        )")
    lines.add("    }")
    lines.add("}")
    return lines.joinToString("\n")
}

private data class Prop(val prop: String, val name: String, val typ: String)

internal fun fileStem(path: String): String {
    val name = File(path).name
    val dot = name.lastIndexOf('.')
    return if (dot < 0) name else name.substring(0, dot)
}

private class FlagError(message: String) : Exception(message)

private class ParsedArgs(
    val packageName: String,
    val output: String,
    val layouts: List<String>,
)

private fun parseArgs(args: Array<String>): ParsedArgs {
    var packageName = ""
    var output = ""
    val positional = mutableListOf<String>()
    var index = 0
    while (index < args.size) {
        val arg = args[index]
        if (arg == "--") {
            positional.addAll(args.drop(index + 1))
            break
        }
        if (!arg.startsWith("-") || arg == "-") {
            positional.addAll(args.drop(index))
            break
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
        if (name == "package" || name == "output") {
            val resolved = value ?: run {
                if (index + 1 >= args.size) {
                    throw FlagError("flag needs an argument: -$name")
                }
                index++
                args[index]
            }
            if (name == "package") {
                packageName = resolved
            } else {
                output = resolved
            }
        } else {
            throw FlagError("flag provided but not defined: -$name")
        }
        index++
    }
    return ParsedArgs(packageName, output, positional)
}

fun run(args: Array<String>, stdout: Appendable, stderr: Appendable): Int {
    val parsed = try {
        parseArgs(args)
    } catch (error: FlagError) {
        stderr.line(error.message ?: error.toString())
        return 2
    }
    if (parsed.packageName.isEmpty() || parsed.output.isEmpty()) {
        stderr.line("gen_view_binding: --package and --output are required")
        return 2
    }
    val layouts = parsed.layouts.sorted()

    val body = mutableListOf<String>()
    for (layout in layouts) {
        val rendered = try {
            render(layout)
        } catch (error: Exception) {
            stderr.line(error.message ?: error.toString())
            return 1
        }
        body.add(rendered)
    }

    val header = listOf(
        "// Generated by //platform/android:gen_view_binding. Do not edit.",
        "@file:Suppress(\"unused\")",
        "",
        "package ${parsed.packageName}.databinding",
        "",
        "import android.view.LayoutInflater",
        "import android.view.View",
        "import android.view.ViewGroup",
        "import dev.proflyder.home.core.ui.ViewBinding",
        "import ${parsed.packageName}.R",
        "",
    )
    val content = header.joinToString("\n") + "\n" + body.joinToString("\n\n") + "\n"
    val output = File(parsed.output)
    try {
        output.writeText(content, Charsets.UTF_8)
    } catch (error: Exception) {
        stderr.line(error.message ?: error.toString())
        return 1
    }
    return 0
}

fun main(args: Array<String>) {
    System.exit(run(args, System.out, System.err))
}
