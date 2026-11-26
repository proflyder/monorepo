package dev.proflyder.home.feature.secrets

import dev.proflyder.home.feature.secrets.presentation.SecretPathRow
import dev.proflyder.home.feature.secrets.presentation.SecretValueText
import dev.proflyder.home.feature.secrets.presentation.keyRows
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SecretValueTextTest {
    @Test
    fun jsonPreviewsAsOneCompactLineAndExpandsFormatted() {
        val raw = "{\n  \"user\": \"svc\",\n  \"ids\": [1, 2]\n}"
        assertEquals("{\"user\":\"svc\",\"ids\":[1,2]}", SecretValueText.preview(raw))
        assertEquals("{\n    \"user\": \"svc\",\n    \"ids\": [\n        1,\n        2\n    ]\n}", SecretValueText.full(raw))
    }

    @Test
    fun otherMultilineValuesCollapseInThePreviewAndKeepTheirLinesWhenExpanded() {
        val pem = "-----BEGIN KEY-----\nabc\ndef\n-----END KEY-----"
        assertEquals("-----BEGIN KEY----- abc def -----END KEY-----", SecretValueText.preview(pem))
        assertEquals(pem, SecretValueText.full(pem))
    }

    @Test
    fun aPlainValueHasNoExpandedBody() {
        assertEquals("s3cr3t", SecretValueText.preview("s3cr3t"))
        assertNull(SecretValueText.full("s3cr3t"))
        assertNull(SecretValueText.full("42"))
    }

    @Test
    fun onlyTheRevealedKeyGetsABody() {
        val rows = keyRows(listOf("a", "b"), revealedKey = "b", revealedValue = "[1]", version = 1, revealingKey = null)
        assertEquals(listOf("key:a", "key:b", "value:b"), rows.map { it.key })
        assertEquals("[\n    1\n]", (rows.last() as SecretPathRow.ValueBody).text)
    }
}
