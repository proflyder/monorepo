package dev.proflyder.home.feature.secrets

import org.junit.runner.RunWith
import org.junit.runners.Suite

@RunWith(Suite::class)
@Suite.SuiteClasses(
    SecretProblemTest::class,
    AndroidDeviceSignerTest::class,
    StagingAliasStateTest::class,
    SecretsCatalogViewModelTest::class,
    SecretPathViewModelTest::class,
    SecretsDeviceViewModelTest::class,
    RemoteSecretsRepositoryTest::class,
    SecretClipboardTest::class,
    SecretsScreenSecureTest::class,
    SecretsScreenshotTest::class,
)
class AllSecretsTests
