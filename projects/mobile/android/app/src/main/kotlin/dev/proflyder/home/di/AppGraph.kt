package dev.proflyder.home.di

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.lifecycle.ViewModelProvider
import dev.proflyder.home.MainActivity
import dev.proflyder.home.core.navigation.FeatureEntry
import dev.proflyder.home.core.navigation.FeatureRegistry
import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.core.runtime.IoDispatcher
import dev.proflyder.home.core.settings.HomeDataStore
import dev.proflyder.home.core.settings.KeystoreTokenCipher
import dev.proflyder.home.core.settings.TokenCipher
import dev.proflyder.home.core.theme.AppearanceController
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.BindingContainer
import dev.zacsweers.metro.DependencyGraph
import dev.zacsweers.metro.Provides
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.plus

@DependencyGraph(
    scope = AppScope::class,
    bindingContainers = [AppBindings::class],
)
interface AppGraph {
    val appearance: AppearanceController
    val startables: Set<AppStartable>

    val viewModelFactory: ViewModelProvider.Factory

    fun inject(activity: MainActivity)

    @DependencyGraph.Factory
    fun interface Factory {
        fun create(@Provides context: Context): AppGraph
    }
}

@BindingContainer
object AppBindings {
    @Provides
    @SingleIn(AppScope::class)
    fun provideDataStore(
        context: Context,
        scope: CoroutineScope,
        @IoDispatcher io: CoroutineDispatcher,
    ): DataStore<Preferences> = HomeDataStore.create(context, scope + io)

    @Provides
    @SingleIn(AppScope::class)
    fun provideTokenCipher(): TokenCipher = KeystoreTokenCipher()

    @Provides
    @SingleIn(AppScope::class)
    fun provideFeatureRegistry(entries: Set<FeatureEntry>): FeatureRegistry =
        FeatureRegistry(entries)
}
