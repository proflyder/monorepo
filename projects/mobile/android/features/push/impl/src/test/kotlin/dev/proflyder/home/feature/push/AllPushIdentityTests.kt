package dev.proflyder.home.feature.push

import org.junit.runner.RunWith
import org.junit.runners.Suite

/** Device identity, registration payloads and name persistence in one target. */
@RunWith(Suite::class)
@Suite.SuiteClasses(
    PushDeviceIdentityTest::class,
    PushRegistrationPublisherTest::class,
    PushRegistrationStoreNameTest::class,
)
class AllPushIdentityTests
