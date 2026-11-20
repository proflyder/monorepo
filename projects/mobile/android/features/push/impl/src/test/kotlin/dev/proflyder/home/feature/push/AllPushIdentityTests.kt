package dev.proflyder.home.feature.push

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    PushDeviceIdentityTest::class,
    PushRegistrationPublisherTest::class,
    PushRegistrationStoreNameTest::class,
)
class AllPushIdentityTests
