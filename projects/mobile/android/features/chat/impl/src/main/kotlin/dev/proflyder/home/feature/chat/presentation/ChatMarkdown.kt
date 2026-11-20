package dev.proflyder.home.feature.chat.presentation

import android.graphics.Typeface
import android.text.SpannableStringBuilder
import android.text.Spanned
import android.text.style.BackgroundColorSpan
import android.text.style.ForegroundColorSpan
import android.text.style.LeadingMarginSpan
import android.text.style.RelativeSizeSpan
import android.text.style.StyleSpan
import android.text.style.TypefaceSpan
import android.text.style.URLSpan

internal data class MarkdownStyle(
    val codeBackground: Int,
    val codeText: Int,
    val linkColor: Int,
    val listIndent: Int,
)

internal class ChatMarkdown(private val style: MarkdownStyle) {
    fun render(source: String): CharSequence {
        val out = SpannableStringBuilder()
        var fence: StringBuilder? = null
        source.lines().forEach { line ->
            fence = consume(out, line, fence)
        }
        fence?.let { appendCode(out, it.toString()) }
        return out.trimEnd()
    }

    private fun consume(out: SpannableStringBuilder, line: String, fence: StringBuilder?): StringBuilder? = when {
        line.trimStart().startsWith(FENCE) && fence == null -> StringBuilder()
        line.trimStart().startsWith(FENCE) -> null.also { appendCode(out, fence.toString()) }
        fence != null -> fence.also { appendFenceLine(it, line) }
        else -> null.also { appendLine(out, line) }
    }

    private fun appendFenceLine(fence: StringBuilder, line: String) {
        if (fence.isNotEmpty()) fence.append('\n')
        fence.append(line)
    }

    private fun appendCode(out: SpannableStringBuilder, code: String) {
        val start = out.length
        out.append(code)
        codeSpans(out, start, out.length)
        out.append('\n')
    }

    private fun appendLine(out: SpannableStringBuilder, raw: String) {
        val heading = HEADING.find(raw)
        val bullet = BULLET.find(raw)
        val numbered = NUMBERED.find(raw)
        val start = out.length
        when {
            heading != null -> appendHeading(out, raw.substring(heading.range.last + 1))
            bullet != null -> appendItem(out, BULLET_MARK, raw.substring(bullet.range.last + 1))
            numbered != null -> appendItem(out, "${numbered.groupValues[1]}. ", raw.substring(numbered.range.last + 1))
            else -> appendInline(out, raw)
        }
        if (bullet != null || numbered != null) {
            out.setSpan(LeadingMarginSpan.Standard(0, style.listIndent), start, out.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        }
        out.append('\n')
    }

    private fun appendHeading(out: SpannableStringBuilder, text: String) {
        val start = out.length
        appendInline(out, text)
        out.setSpan(StyleSpan(Typeface.BOLD), start, out.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        out.setSpan(RelativeSizeSpan(HEADING_SCALE), start, out.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
    }

    private fun appendItem(out: SpannableStringBuilder, mark: String, text: String) {
        out.append(mark)
        appendInline(out, text)
    }

    private fun appendInline(out: SpannableStringBuilder, text: String) {
        var cursor = 0
        INLINE.findAll(text).forEach { match ->
            appendLinked(out, text.substring(cursor, match.range.first))
            appendToken(out, match)
            cursor = match.range.last + 1
        }
        appendLinked(out, text.substring(cursor))
    }

    private fun appendToken(out: SpannableStringBuilder, match: MatchResult) {
        val value = match.value
        val start = out.length
        when {
            value.startsWith(CODE_MARK) -> codeSpans(out.append(value.trim('`')), start, out.length)
            value.startsWith("**") || value.startsWith("__") ->
                styled(out, value.substring(2, value.length - 2), Typeface.BOLD)
            value.startsWith("[") -> appendLink(out, match.groupValues[LINK_TEXT], match.groupValues[LINK_URL])
            else -> styled(out, value.substring(1, value.length - 1), Typeface.ITALIC)
        }
    }

    private fun styled(out: SpannableStringBuilder, text: String, typeface: Int) {
        val start = out.length
        out.append(text)
        out.setSpan(StyleSpan(typeface), start, out.length, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
    }

    private fun appendLink(out: SpannableStringBuilder, text: String, url: String) {
        val start = out.length
        out.append(text)
        linkSpans(out, url, start, out.length)
    }

    private fun appendLinked(out: SpannableStringBuilder, text: String) {
        var cursor = 0
        URL.findAll(text).forEach { match ->
            out.append(text.substring(cursor, match.range.first))
            appendLink(out, match.value, match.value)
            cursor = match.range.last + 1
        }
        out.append(text.substring(cursor))
    }

    private fun linkSpans(out: SpannableStringBuilder, url: String, start: Int, end: Int) {
        out.setSpan(URLSpan(url), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        out.setSpan(ForegroundColorSpan(style.linkColor), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
    }

    private fun codeSpans(out: SpannableStringBuilder, start: Int, end: Int) {
        out.setSpan(TypefaceSpan(MONOSPACE), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        out.setSpan(BackgroundColorSpan(style.codeBackground), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        out.setSpan(ForegroundColorSpan(style.codeText), start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
    }

    private fun SpannableStringBuilder.trimEnd(): CharSequence {
        var end = length
        while (end > 0 && this[end - 1].isWhitespace()) end--
        return subSequence(0, end)
    }

    private companion object {
        const val FENCE = "```"
        const val CODE_MARK = "`"
        const val MONOSPACE = "monospace"
        const val BULLET_MARK = "•  "
        const val HEADING_SCALE = 1.1f
        const val LINK_TEXT = 2
        const val LINK_URL = 3
        val HEADING = Regex("^\\s{0,3}#{1,6}\\s+")
        val BULLET = Regex("^\\s*[-*•]\\s+")
        val NUMBERED = Regex("^\\s*(\\d{1,3})[.)]\\s+")
        val INLINE = Regex(
            "`[^`]+`|\\*\\*[^*]+\\*\\*|__[^_]+__|(\\[([^\\]]+)\\]\\((https?://[^)\\s]+)\\))|\\*[^*\\s][^*]*\\*|_[^_\\s][^_]*_",
        )
        val URL = Regex("https?://[^\\s)]+")
    }
}
