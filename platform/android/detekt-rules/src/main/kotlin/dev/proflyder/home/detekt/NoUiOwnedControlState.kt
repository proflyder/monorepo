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
import org.jetbrains.kotlin.psi.KtElement
import org.jetbrains.kotlin.psi.KtLambdaExpression
import org.jetbrains.kotlin.psi.KtPrefixExpression
import org.jetbrains.kotlin.psi.psiUtil.getStrictParentOfType

class NoUiOwnedControlState(config: Config = Config.empty) : Rule(config) {
    override val issue: Issue = Issue(
        id = javaClass.simpleName,
        description = "Control state comes from the ViewModel; the UI only sends intents.",
        severity = Severity.Maintainability,
        debt = Debt.TEN_MINS,
    )

    override fun visitBinaryExpression(expression: KtBinaryExpression) {
        super.visitBinaryExpression(expression)
        if (expression.operationToken != KtTokens.EQ) return
        val left = expression.left ?: return
        if (propertyName(left) !in controlProperties) return
        if (!flipsItself(left.text, expression) && !insideListener(expression)) return
        reportAt(expression)
    }

    override fun visitCallExpression(expression: KtCallExpression) {
        super.visitCallExpression(expression)
        if (expression.calleeExpression?.text != "toggle") return
        if (expression.valueArguments.isNotEmpty() || expression.lambdaArguments.isNotEmpty()) return
        val target = expression.parent as? KtDotQualifiedExpression ?: return
        if (target.selectorExpression != expression) return
        if (target.receiverExpression.text.endsWith("viewModel", ignoreCase = true)) return
        reportAt(target)
    }

    private fun propertyName(target: KtElement): String =
        (target as? KtDotQualifiedExpression)?.selectorExpression?.text ?: target.text

    private fun flipsItself(target: String, expression: KtBinaryExpression): Boolean {
        val right = expression.right as? KtPrefixExpression ?: return false
        return right.operationToken == KtTokens.EXCL && right.baseExpression?.text == target
    }

    private fun insideListener(expression: KtBinaryExpression): Boolean {
        val lambda = expression.getStrictParentOfType<KtLambdaExpression>() ?: return false
        val call = lambda.getStrictParentOfType<KtCallExpression>() ?: return false
        return listenerCall.matches(call.calleeExpression?.text.orEmpty())
    }

    private fun reportAt(element: KtElement) {
        report(
            CodeSmell(
                issue,
                Entity.from(element),
                "Send an intent to the ViewModel and render the control from its StateFlow instead of changing it here.",
            ),
        )
    }

    private companion object {
        val controlProperties = setOf("isChecked", "isSelected", "isActivated", "checked")
        val listenerCall = Regex("(set|add)On\\w*Listener|doOn\\w*|setOnClickListener")
    }
}
