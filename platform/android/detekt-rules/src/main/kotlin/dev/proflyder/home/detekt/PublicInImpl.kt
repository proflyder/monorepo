package dev.proflyder.home.detekt

import io.gitlab.arturbosch.detekt.api.CodeSmell
import io.gitlab.arturbosch.detekt.api.Config
import io.gitlab.arturbosch.detekt.api.Debt
import io.gitlab.arturbosch.detekt.api.Entity
import io.gitlab.arturbosch.detekt.api.Issue
import io.gitlab.arturbosch.detekt.api.Rule
import io.gitlab.arturbosch.detekt.api.Severity
import org.jetbrains.kotlin.lexer.KtTokens
import org.jetbrains.kotlin.psi.KtDeclaration
import org.jetbrains.kotlin.psi.KtFile
import org.jetbrains.kotlin.psi.KtNamedDeclaration

class PublicInImpl(config: Config = Config.empty) : Rule(config) {
    override val issue: Issue = Issue(
        id = javaClass.simpleName,
        description = "Top-level declarations of a feature impl module are internal.",
        severity = Severity.Maintainability,
        debt = Debt.FIVE_MINS,
    )

    private val implPathMarker: String by lazy { valueOrDefault("implPathMarker", "/impl/src/main/") }

    override fun visitKtFile(file: KtFile) {
        super.visitKtFile(file)
        val path = file.virtualFilePath.replace('\\', '/')
        if (!path.contains(implPathMarker)) return
        file.declarations.filter(::isVisibleOutsideModule).forEach(::reportDeclaration)
    }

    private fun isVisibleOutsideModule(declaration: KtDeclaration): Boolean =
        restrictingModifiers.none(declaration::hasModifier)

    private fun reportDeclaration(declaration: KtDeclaration) {
        report(
            CodeSmell(
                issue,
                (declaration as? KtNamedDeclaration)?.let(Entity::atName) ?: Entity.from(declaration),
                "Mark ${declaration.name} internal; impl exposes nothing beyond its module.",
            ),
        )
    }

    private companion object {
        val restrictingModifiers = listOf(KtTokens.INTERNAL_KEYWORD, KtTokens.PRIVATE_KEYWORD)
    }
}
