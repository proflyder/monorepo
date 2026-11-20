package dev.proflyder.home.detekt

import io.gitlab.arturbosch.detekt.api.CodeSmell
import io.gitlab.arturbosch.detekt.api.Config
import io.gitlab.arturbosch.detekt.api.Debt
import io.gitlab.arturbosch.detekt.api.Entity
import io.gitlab.arturbosch.detekt.api.Issue
import io.gitlab.arturbosch.detekt.api.Rule
import io.gitlab.arturbosch.detekt.api.Severity
import org.jetbrains.kotlin.psi.KtTryExpression

class NoTry(config: Config = Config.empty) : Rule(config) {
    override val issue: Issue = Issue(
        id = javaClass.simpleName,
        description = "try/catch/finally is forbidden; model failures with runCatching or use { }.",
        severity = Severity.CodeSmell,
        debt = Debt.TWENTY_MINS,
    )

    override fun visitTryExpression(expression: KtTryExpression) {
        super.visitTryExpression(expression)
        report(
            CodeSmell(
                issue,
                Entity.from(expression),
                "Replace try/catch with runCatching or suspendRunCatching; use use { } for resources.",
            ),
        )
    }
}
