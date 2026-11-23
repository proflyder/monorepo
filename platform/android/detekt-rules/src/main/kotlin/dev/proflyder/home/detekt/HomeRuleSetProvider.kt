package dev.proflyder.home.detekt

import io.gitlab.arturbosch.detekt.api.Config
import io.gitlab.arturbosch.detekt.api.RuleSet
import io.gitlab.arturbosch.detekt.api.RuleSetProvider

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
            NoDirectStateWrite(config),
            PublicInImpl(config),
            NoUiOwnedControlState(config),
            NoCodeLayoutManager(config),
        ),
    )
}
