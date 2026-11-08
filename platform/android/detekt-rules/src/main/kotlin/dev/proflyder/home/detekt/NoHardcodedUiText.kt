package dev.proflyder.home.detekt

import io.gitlab.arturbosch.detekt.api.CodeSmell
import io.gitlab.arturbosch.detekt.api.Config
import io.gitlab.arturbosch.detekt.api.Debt
import io.gitlab.arturbosch.detekt.api.Entity
import io.gitlab.arturbosch.detekt.api.Issue
import io.gitlab.arturbosch.detekt.api.Rule
import io.gitlab.arturbosch.detekt.api.Severity
import org.jetbrains.kotlin.lexer.KtTokens
import org.jetbrains.kotlin.psi.KtBinaryExpression
import org.jetbrains.kotlin.psi.KtCallExpression
import org.jetbrains.kotlin.psi.KtDotQualifiedExpression
import org.jetbrains.kotlin.psi.KtEscapeStringTemplateEntry
import org.jetbrains.kotlin.psi.KtExpression
import org.jetbrains.kotlin.psi.KtLiteralStringTemplateEntry
import org.jetbrains.kotlin.psi.KtStringTemplateExpression

/**
 * Reports user-facing text passed as a string literal to `setText`, `Toast`,
 * `Snackbar` or assigned to a `.text` property; it belongs in `strings.xml`
 * (English + `values-ru`).
 *
 * Android Lint's `HardcodedText` covers XML layouts; this rule covers the
 * Kotlin call sites it cannot see. Empty strings (`setText("")`, used to clear
 * a field) and templates with interpolations are not user-facing literals.
 */
class NoHardcodedUiText(config: Config = Config.empty) : Rule(config) {
    override val issue: Issue = Issue(
        id = javaClass.simpleName,
        description = "User-facing text must live in strings.xml.",
        severity = Severity.Style,
        debt = Debt.FIVE_MINS,
    )

    override fun visitCallExpression(expression: KtCallExpression) {
        super.visitCallExpression(expression)
        val argument = uiTextArgument(expression) ?: return
        reportIfLiteral(argument)
    }

    override fun visitBinaryExpression(expression: KtBinaryExpression) {
        super.visitBinaryExpression(expression)
        if (expression.operationToken != KtTokens.EQ) return
        if (!expression.left?.text.orEmpty().endsWith(".text")) return
        val right = expression.right ?: return
        reportIfLiteral(right)
    }

    private fun uiTextArgument(call: KtCallExpression): KtExpression? {
        val callee = call.calleeExpression?.text
        val receiver = (call.parent as? KtDotQualifiedExpression)?.receiverExpression?.text
        val argument = when {
            callee == "makeText" && receiver == "Toast" -> call.valueArguments.getOrNull(1)
            callee == "make" && receiver == "Snackbar" -> call.valueArguments.getOrNull(1)
            callee == "setText" -> call.valueArguments.getOrNull(0)
            else -> null
        }
        return argument?.getArgumentExpression()
    }

    private fun reportIfLiteral(expression: KtExpression) {
        val literal = expression as? KtStringTemplateExpression ?: return
        if (literal.entries.any { it !is KtLiteralStringTemplateEntry && it !is KtEscapeStringTemplateEntry }) {
            return
        }
        val value = literal.entries
            .joinToString("") { it.text }
            .trim()
        if (value.isEmpty()) return
        report(
            CodeSmell(
                issue,
                Entity.from(expression),
                "Move this user-facing text into strings.xml (English and values-ru).",
            ),
        )
    }
}
