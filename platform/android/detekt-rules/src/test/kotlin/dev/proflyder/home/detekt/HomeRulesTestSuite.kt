package dev.proflyder.home.detekt

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    NoElseIfTest::class,
    NoTryTest::class,
    LongLambdaTest::class,
    NoInlineJsonInTestsTest::class,
    NoHardcodedUiTextTest::class,
)
class HomeRulesTestSuite
