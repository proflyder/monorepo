package dev.proflyder.home.detekt

import io.gitlab.arturbosch.detekt.api.CodeSmell
import io.gitlab.arturbosch.detekt.api.Config
import io.gitlab.arturbosch.detekt.api.Debt
import io.gitlab.arturbosch.detekt.api.Entity
import io.gitlab.arturbosch.detekt.api.Issue
import io.gitlab.arturbosch.detekt.api.Rule
import io.gitlab.arturbosch.detekt.api.Severity
import org.jetbrains.kotlin.lexer.KtTokens
import org.jetbrains.kotlin.psi.KtAnnotated
import org.jetbrains.kotlin.psi.KtClass
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

    private val layoutViewSupertypes: Set<String> by lazy {
        valueOrDefault("layoutViewSupertypes", defaultLayoutViewSupertypes).toSet()
    }

    private val implPathMarker: String by lazy { valueOrDefault("implPathMarker", "/impl/src/main/") }

    override fun visitKtFile(file: KtFile) {
        super.visitKtFile(file)
        val path = file.virtualFilePath.replace('\\', '/')
        if (!path.contains(implPathMarker)) return
        file.declarations.filter(::isVisibleOutsideModule).forEach(::reportDeclaration)
    }

    private fun isVisibleOutsideModule(declaration: KtDeclaration): Boolean =
        restrictingModifiers.none(declaration::hasModifier) && !isGraphContribution(declaration) && !isLayoutView(declaration)

    private fun isGraphContribution(declaration: KtDeclaration): Boolean =
        declaration.hasAnnotation(graphContributionAnnotations) || hasMemberInjection(declaration)

    private fun isLayoutView(declaration: KtDeclaration): Boolean =
        (declaration as? KtClass)?.superTypeListEntries.orEmpty().any { it.typeAsUserType?.referencedName in layoutViewSupertypes }

    private fun hasMemberInjection(declaration: KtDeclaration): Boolean =
        (declaration as? KtClass)?.getProperties().orEmpty().any { it.hasAnnotation(memberInjectionAnnotations) }

    private fun KtAnnotated.hasAnnotation(names: Set<String>): Boolean =
        annotationEntries.any { it.shortName?.asString() in names }

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
        val defaultLayoutViewSupertypes = listOf("View", "ViewGroup", "FrameLayout", "LinearLayout", "ConstraintLayout")
        val memberInjectionAnnotations = setOf("Inject")
        val graphContributionAnnotations = setOf(
            "Inject",
            "ContributesBinding",
            "ContributesIntoSet",
            "ContributesIntoMap",
            "ContributesTo",
            "BindingContainer",
            "GraphExtension",
        )
    }
}
