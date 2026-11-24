package dev.proflyder.home.feature.update

import dev.proflyder.home.core.navigation.AppActivityContribution
import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.feature.update.data.AppUpdater
import dev.proflyder.home.feature.update.data.DataStoreUpdateCheckStore
import dev.proflyder.home.feature.update.data.FirebaseAppUpdater
import dev.proflyder.home.feature.update.data.UpdateCheckStore
import dev.proflyder.home.feature.update.presentation.UpdatePromptContribution
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.BindingContainer
import dev.zacsweers.metro.Binds
import dev.zacsweers.metro.ContributesTo
import dev.zacsweers.metro.IntoSet

@ContributesTo(AppScope::class)
@BindingContainer
abstract class UpdateBindings {
    @Binds
    internal abstract fun center(impl: UpdateCoordinator): UpdateCenter

    @Binds
    internal abstract fun updater(impl: FirebaseAppUpdater): AppUpdater

    @Binds
    internal abstract fun checks(impl: DataStoreUpdateCheckStore): UpdateCheckStore

    @Binds
    @IntoSet
    internal abstract fun startables(impl: UpdateCoordinator): AppStartable

    @Binds
    @IntoSet
    internal abstract fun prompts(impl: UpdatePromptContribution): AppActivityContribution
}
