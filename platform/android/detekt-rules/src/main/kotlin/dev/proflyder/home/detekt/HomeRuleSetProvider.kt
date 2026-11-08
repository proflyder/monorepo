package dev.proflyder.home.detekt

import io.gitlab.arturbosch.detekt.api.Config
import io.gitlab.arturbosch.detekt.api.RuleSet
import io.gitlab.arturbosch.detekt.api.RuleSetProvider

/**
 * First-party rules that encode the Home Android code conventions.
 *
 * The provider is registered through
 * `META-INF/services/io.gitlab.arturbosch.detekt.api.RuleSetProvider`, so the
 * detekt CLI picks it up when the rules jar is passed with `--plugins`.
 */
class HomeRuleSetProvider : RuleSetProvider {
    override val ruleSetId: String = "home-rules"

    override fun instance(config: Config): RuleSet = RuleSet(
        ruleSetId,
        listOf(
            NoElseIf(config),
            NoTry(config),
            LongLambda(config),
            NoInlineJsonInTests(config),
            NoHardcodedUiText(config),
        ),
    )
}
