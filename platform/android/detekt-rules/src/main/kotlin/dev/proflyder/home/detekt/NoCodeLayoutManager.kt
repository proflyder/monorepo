package dev.proflyder.home.detekt

import io.gitlab.arturbosch.detekt.api.CodeSmell
import io.gitlab.arturbosch.detekt.api.Config
import io.gitlab.arturbosch.detekt.api.Debt
import io.gitlab.arturbosch.detekt.api.Entity
import io.gitlab.arturbosch.detekt.api.Issue
import io.gitlab.arturbosch.detekt.api.Rule
import io.gitlab.arturbosch.detekt.api.Severity
import org.jetbrains.kotlin.psi.KtBinaryExpression
import org.jetbrains.kotlin.psi.KtCallExpression
import org.jetbrains.kotlin.psi.KtDotQualifiedExpression
import org.jetbrains.kotlin.psi.KtElement

class NoCodeLayoutManager(config: Config = Config.empty) : Rule(config) {
    override val issue: Issue = Issue(
        id = javaClass.simpleName,
        description = "Simple RecyclerView lists declare app:layoutManager and app:spanCount in XML.",
        severity = Severity.Maintainability,
        debt = Debt.FIVE_MINS,
    )

    override fun visitCallExpression(expression: KtCallExpression) {
        super.visitCallExpression(expression)
        val callee = expression.calleeExpression?.text ?: return
        if (callee !in managers) return
        if (configuresBehaviour(expression)) return
        reportAt(expression)
    }

    override fun visitBinaryExpression(expression: KtBinaryExpression) {
        super.visitBinaryExpression(expression)
        if (expression.operationReference.text != "=") return
        val target = (expression.left as? KtDotQualifiedExpression)?.selectorExpression?.text ?: return
        if (target != "layoutManager") return
        val created = expression.right ?: return
        if (created.text.contains(behaviourMarker)) return
        reportAt(expression)
    }

    override fun visitDotQualifiedExpression(expression: KtDotQualifiedExpression) {
        super.visitDotQualifiedExpression(expression)
        val call = expression.selectorExpression as? KtCallExpression ?: return
        if (call.calleeExpression?.text == "setLayoutManager") reportAt(expression)
    }

    private fun configuresBehaviour(call: KtCallExpression): Boolean {
        val qualified = call.parent as? KtDotQualifiedExpression ?: return false
        return qualified.text.contains(behaviourMarker)
    }

    private fun reportAt(element: KtElement) {
        report(
            CodeSmell(
                issue,
                Entity.from(element),
                "Declare the layout manager in XML (app:layoutManager, app:spanCount, app:stackFromEnd); " +
                    "code is allowed only when it installs a SpanSizeLookup.",
            ),
        )
    }

    private companion object {
        val managers = setOf("LinearLayoutManager", "GridLayoutManager")
        val behaviourMarker = Regex("spanSizeLookup")
    }
}
