package dev.proflyder.home.detekt

import io.gitlab.arturbosch.detekt.api.CodeSmell
import io.gitlab.arturbosch.detekt.api.Config
import io.gitlab.arturbosch.detekt.api.Debt
import io.gitlab.arturbosch.detekt.api.Entity
import io.gitlab.arturbosch.detekt.api.Issue
import io.gitlab.arturbosch.detekt.api.Rule
import io.gitlab.arturbosch.detekt.api.Severity
import org.jetbrains.kotlin.psi.KtCallExpression
import org.jetbrains.kotlin.psi.KtLambdaArgument
import org.jetbrains.kotlin.psi.KtLambdaExpression
import org.jetbrains.kotlin.psi.KtValueArgument
import org.jetbrains.kotlin.psi.KtValueArgumentList

class LongLambda(config: Config = Config.empty) : Rule(config) {
    override val issue: Issue = Issue(
        id = javaClass.simpleName,
        description = "A lambda body longer than a few lines should be a named function.",
        severity = Severity.Maintainability,
        debt = Debt.TEN_MINS,
    )

    private val maxLines: Int = valueOrDefault("maxLines", 3)

    override fun visitLambdaExpression(lambdaExpression: KtLambdaExpression) {
        super.visitLambdaExpression(lambdaExpression)
        val block = lambdaExpression.bodyExpression ?: return
        if (lambdaExpression.isScopeBuilderLambda()) return
        if (block.statements.size <= maxLines) return
        report(
            CodeSmell(
                issue,
                Entity.from(lambdaExpression),
                "Extract this lambda body into a named function (max $maxLines statements).",
            ),
        )
    }

    private fun KtLambdaExpression.isScopeBuilderLambda(): Boolean {
        val name = calleeName() ?: return false
        return name in SCOPE_FUNCTIONS || name.startsWith("build")
    }

    private fun KtLambdaExpression.calleeName(): String? {
        val argument = parent
        val call = when (argument) {
            is KtLambdaArgument -> argument.parent as? KtCallExpression
            is KtValueArgument ->
                (argument.parent as? KtValueArgumentList)?.parent as? KtCallExpression
            else -> null
        }
        return call?.calleeExpression?.text
    }

    private companion object {
        val SCOPE_FUNCTIONS = setOf("apply", "with", "runTest", "runBlocking")
    }
}
