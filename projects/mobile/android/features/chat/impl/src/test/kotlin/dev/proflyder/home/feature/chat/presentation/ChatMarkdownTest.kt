package dev.proflyder.home.feature.chat.presentation

import android.graphics.Typeface
import android.text.Spanned
import android.text.style.LeadingMarginSpan
import android.text.style.StyleSpan
import android.text.style.TypefaceSpan
import android.text.style.URLSpan
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

@RunWith(RobolectricTestRunner::class)
@Config(sdk = [35])
class ChatMarkdownTest {
    private val markdown = ChatMarkdown(MarkdownStyle(codeBackground = 1, codeText = 2, linkColor = 3, listIndent = 4))

    private fun render(source: String): Spanned = markdown.render(source) as Spanned

    private inline fun <reified T> Spanned.spanned(): List<String> =
        getSpans(0, length, T::class.java).map { subSequence(getSpanStart(it), getSpanEnd(it)).toString() }

    @Test
    fun inlineMarksBecomeSpans() {
        val text = render("Это **жирно**, это *курсив*, а это `код` и [ссылка](https://example.com/a).")
        assertEquals("Это жирно, это курсив, а это код и ссылка.", text.toString())
        val styles = text.getSpans(0, text.length, StyleSpan::class.java).map { it.style }
        assertEquals(listOf(Typeface.BOLD, Typeface.ITALIC), styles)
        assertEquals(listOf("код"), text.spanned<TypefaceSpan>())
        assertEquals("https://example.com/a", text.getSpans(0, text.length, URLSpan::class.java).single().url)
    }

    @Test
    fun blocksListsAndBareLinksAreRendered() {
        val text = render("# Отчёт\n- первый\n2. второй\n```\nkubectl get pods\n```\nсм. https://grafana.example.com/d")
        assertEquals("Отчёт\n•  первый\n2. второй\nkubectl get pods\nсм. https://grafana.example.com/d", text.toString())
        assertEquals(listOf("kubectl get pods"), text.spanned<TypefaceSpan>())
        assertEquals(2, text.getSpans(0, text.length, LeadingMarginSpan::class.java).size)
        assertEquals(listOf("https://grafana.example.com/d"), text.spanned<URLSpan>())
    }
}
