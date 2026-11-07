package dev.proflyder.home.tools.genviewbinding

import java.io.File
import java.nio.file.Files
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

private const val LAYOUT_FIXTURE = """<?xml version="1.0" encoding="utf-8"?>
<LinearLayout xmlns:android="http://schemas.android.com/apk/res/android"
    android:layout_width="match_parent" android:layout_height="match_parent">
    <TextView android:id="@+id/item_title" />
    <View android:id="@id/divider" />
    <com.google.android.material.button.MaterialButton android:id="@+id/item_save" />
    <TextView android:id="@+id/item_title" />
    <TextView android:id="@+id/root" />
    <include layout="@layout/other" android:id="@+id/included" />
    <TextView android:text="no id" />
</LinearLayout>
"""

class GenViewBindingTest {
    private fun generate(layouts: Map<String, String>): Triple<Int, String, String> {
        val temp = Files.createTempDirectory("gen-view-binding-").toFile()
        val paths = layouts.map { (name, body) ->
            File(temp, name).apply { writeText(body, Charsets.UTF_8) }
        }
        val output = File(temp, "out.kt")
        val stdout = StringBuilder()
        val stderr = StringBuilder()
        val args = arrayOf("--package", "a.b", "--output", output.path) + paths.map { it.path }
        val code = run(args, stdout, stderr)
        val text = if (output.exists()) output.readText(Charsets.UTF_8) else ""
        return Triple(code, text, stderr.toString())
    }

    @Test
    fun generatesTypedProperties() {
        val (code, kt, stderr) = generate(mapOf("fragment_sample.xml" to LAYOUT_FIXTURE))
        assertEquals("exit $code: $stderr", 0, code)
        for (want in listOf(
            "package a.b.databinding",
            "import a.b.R",
            "class FragmentSampleBinding private constructor(",
            "override val root: android.widget.LinearLayout,",
            "val itemTitle: android.widget.TextView,",
            "val divider: android.view.View,",
            "val itemSave: com.google.android.material.button.MaterialButton,",
            "val included: android.view.View,",
            "R.layout.fragment_sample",
        )) {
            assertTrue("output does not contain \"$want\":\n$kt", kt.contains(want))
        }
    }

    @Test
    fun duplicateIdsAndRootNameClash() {
        val (_, kt, _) = generate(mapOf("fragment_sample.xml" to LAYOUT_FIXTURE))
        assertEquals("itemTitle declared more than once:\n$kt", 1, countOccurrences(kt, "val itemTitle:"))
        assertTrue("root name clash not renamed:\n$kt", kt.contains("val rootView: android.widget.TextView,"))
    }

    @Test
    fun oneClassPerLayoutSorted() {
        val (_, kt, _) = generate(
            mapOf(
                "b_two.xml" to LAYOUT_FIXTURE,
                "a_one.xml" to LAYOUT_FIXTURE,
            ),
        )
        val a = kt.indexOf("class AOneBinding")
        val b = kt.indexOf("class BTwoBinding")
        assertTrue("layouts not sorted (a=$a b=$b):\n$kt", a >= 0 && b >= 0 && a < b)
    }

    @Test
    fun mergeRootIsRejected() {
        val (code, _, stderr) = generate(
            mapOf(
                "x.xml" to """<merge xmlns:android="http://schemas.android.com/apk/res/android"/>""",
            ),
        )
        assertTrue("merge root accepted", code != 0)
        assertTrue("missing merge error: $stderr", stderr.contains("<merge> roots are not supported"))
    }

    private fun countOccurrences(text: String, needle: String): Int {
        var count = 0
        var index = text.indexOf(needle)
        while (index >= 0) {
            count++
            index = text.indexOf(needle, index + needle.length)
        }
        return count
    }
}
