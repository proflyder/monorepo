package dev.proflyder.home.core.data

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    EntityStoreTest::class,
    FeatureScopeTest::class,
)
class AllDataTests
