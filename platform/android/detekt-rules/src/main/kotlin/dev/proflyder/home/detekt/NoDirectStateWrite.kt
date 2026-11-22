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
import org.jetbrains.kotlin.psi.KtClassOrObject
import org.jetbrains.kotlin.psi.KtDotQualifiedExpression
import org.jetbrains.kotlin.psi.KtExpression
import org.jetbrains.kotlin.psi.KtNamedFunction
import org.jetbrains.kotlin.psi.psiUtil.getStrictParentOfType

class NoDirectStateWrite(config: Config = Config.empty) : Rule(config) {
    override val issue: Issue = Issue(
        id = javaClass.simpleName,
        description = "ViewModel state changes only through updateState.",
        severity = Severity.Maintainability,
        debt = Debt.FIVE_MINS,
    )

    private val stateNames: List<String> by lazy { valueOrDefault("stateNames", listOf("mutableState")) }

    private val viewModelBases: List<String> by lazy { valueOrDefault("viewModelBases", listOf("BaseViewModel")) }

    private val reducerFunction: String by lazy { valueOrDefault("reducerFunction", "updateState") }

    override fun visitBinaryExpression(expression: KtBinaryExpression) {
        super.visitBinaryExpression(expression)
        if (expression.operationToken !in assignmentTokens) return
        val target = expression.left as? KtDotQualifiedExpression ?: return
        if (target.selectorExpression?.text != "value") return
        reportIfState(target, expression)
    }

    override fun visitCallExpression(expression: KtCallExpression) {
        super.visitCallExpression(expression)
        if (expression.calleeExpression?.text !in writeCalls) return
        val target = expression.parent as? KtDotQualifiedExpression ?: return
        if (target.selectorExpression != expression) return
        reportIfState(target, target)
    }

    private fun reportIfState(target: KtDotQualifiedExpression, statement: KtExpression) {
        if (target.receiverExpression.text.removePrefix("this.") !in stateNames) return
        if (target.getStrictParentOfType<KtNamedFunction>()?.name == reducerFunction) return
        if (!insideViewModel(target)) return
        report(
            CodeSmell(
                issue,
                Entity.from(statement),
                "Change state with $reducerFunction { copy(...) } instead of writing ${target.receiverExpression.text} directly.",
            ),
        )
    }

    private fun insideViewModel(target: KtDotQualifiedExpression): Boolean {
        val owner = target.getStrictParentOfType<KtClassOrObject>() ?: return false
        val supertypes = owner.superTypeListEntries.mapNotNull { it.typeReference?.text?.substringBefore('<') }
        return supertypes.any(viewModelBases::contains)
    }

    private companion object {
        val assignmentTokens = setOf(KtTokens.EQ, KtTokens.PLUSEQ, KtTokens.MINUSEQ)
        val writeCalls = setOf("update", "emit", "tryEmit", "compareAndSet", "getAndUpdate", "updateAndGet")
    }
}
