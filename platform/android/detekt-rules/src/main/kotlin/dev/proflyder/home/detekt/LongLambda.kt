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

/**
 * Reports a lambda body with more than `maxLines` statements (default 3); the
 * body belongs in a named function.
 *
 * A single multi-line expression (a `when`, an `if`, or a builder call) is one
 * statement and is not reported: it reads as one unit. The threshold targets
 * callbacks that accumulate unrelated steps.
 *
 * Scope/builder blocks (`apply {}`, `with {}`, `build* {}`) and test bodies
 * (`runTest {}`, `runBlocking {}`) are exempt: the former configure an object,
 * the latter are the body of an already named test function.
 */
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
        // Builders (`buildList`, `buildString`, `buildJsonObject`, ...) and the
        // classic scope functions only configure the receiver; the body is not
        // behaviour worth naming on its own. `runTest`/`runBlocking` wrap an
        // already-named test function, so their body is that function's body.
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
