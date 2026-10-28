package dev.proflyder.home.core.operation

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    IdempotentRequestKeyTest::class,
    EditorCompletionTest::class,
)
class OperationTestSuite
