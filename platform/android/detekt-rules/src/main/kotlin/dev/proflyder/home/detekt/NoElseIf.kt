package dev.proflyder.home.detekt

import io.gitlab.arturbosch.detekt.api.CodeSmell
import io.gitlab.arturbosch.detekt.api.Config
import io.gitlab.arturbosch.detekt.api.Debt
import io.gitlab.arturbosch.detekt.api.Entity
import io.gitlab.arturbosch.detekt.api.Issue
import io.gitlab.arturbosch.detekt.api.Rule
import io.gitlab.arturbosch.detekt.api.Severity
import org.jetbrains.kotlin.psi.KtIfExpression

/**
 * Forbids an `else if` chain: express the same choice with a `when`.
 *
 * `when` keeps every branch at the same indentation level and forces an
 * exhaustive-ish shape instead of a right-leaning if-tree that is hard to scan.
 */
class NoElseIf(config: Config = Config.empty) : Rule(config) {
    override val issue: Issue = Issue(
        id = javaClass.simpleName,
        description = "An else-if chain hides the branch structure; use a when expression.",
        severity = Severity.Style,
        debt = Debt.FIVE_MINS,
    )

    override fun visitIfExpression(expression: KtIfExpression) {
        super.visitIfExpression(expression)
        val elseBranch = expression.`else` ?: return
        if (elseBranch !is KtIfExpression) return
        report(
            CodeSmell(
                issue,
                Entity.from(expression.elseKeyword ?: expression),
                "Rewrite this else-if chain as a when expression.",
            ),
        )
    }
}
