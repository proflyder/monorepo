package dev.proflyder.home.detekt

import io.gitlab.arturbosch.detekt.api.CodeSmell
import io.gitlab.arturbosch.detekt.api.Config
import io.gitlab.arturbosch.detekt.api.Debt
import io.gitlab.arturbosch.detekt.api.Entity
import io.gitlab.arturbosch.detekt.api.Issue
import io.gitlab.arturbosch.detekt.api.Rule
import io.gitlab.arturbosch.detekt.api.Severity
import org.jetbrains.kotlin.psi.KtEscapeStringTemplateEntry
import org.jetbrains.kotlin.psi.KtFile
import org.jetbrains.kotlin.psi.KtLiteralStringTemplateEntry
import org.jetbrains.kotlin.psi.KtStringTemplateExpression

/**
 * Reports JSON literals embedded in test sources; fixtures live in the
 * `src/test/resources/fixtures` directory and are loaded by a shared helper.
 *
 * Inline JSON hides the payload from both tooling and reviewers and makes a
 * test read like a serialization dump. The rule only looks at files under a
 * `src/test/` directory.
 */
class NoInlineJsonInTests(config: Config = Config.empty) : Rule(config) {
    override val issue: Issue = Issue(
        id = javaClass.simpleName,
        description = "JSON must live in a test fixture file, not in a string literal.",
        severity = Severity.Style,
        debt = Debt.TEN_MINS,
    )

    private var testSource: Boolean = false

    override fun visitKtFile(file: KtFile) {
        testSource = file.virtualFilePath
            .replace('\\', '/')
            .contains("/src/test/")
        super.visitKtFile(file)
    }

    override fun visitStringTemplateExpression(expression: KtStringTemplateExpression) {
        super.visitStringTemplateExpression(expression)
        if (!testSource) return
        if (expression.entries.any { entry ->
                entry !is KtLiteralStringTemplateEntry && entry !is KtEscapeStringTemplateEntry
            }
        ) {
            return
        }
        if (!looksLikeJson(expression.text)) return
        report(
            CodeSmell(
                issue,
                Entity.from(expression),
                "Move this JSON payload into src/test/resources/fixtures/*.json and load it.",
            ),
        )
    }

    private fun looksLikeJson(text: String): Boolean {
        val trimmed = text.trim().let {
            if (it.startsWith("\"\"\"")) {
                it.removePrefix("\"\"\"").removeSuffix("\"\"\"")
            } else {
                it.removeSurrounding("\"")
            }
        }
        val objectJson = trimmed.startsWith("{") && trimmed.contains(":")
        val arrayJson = trimmed.startsWith("[") && (trimmed.contains("{") || trimmed.contains(":"))
        return objectJson || arrayJson
    }
}
