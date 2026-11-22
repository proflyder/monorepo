package dev.proflyder.home.core.runtime

import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.model.ServerClock
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.BindingContainer
import dev.zacsweers.metro.ContributesTo
import dev.zacsweers.metro.Multibinds
import dev.zacsweers.metro.Provides
import dev.zacsweers.metro.SingleIn
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

@ContributesTo(AppScope::class)
@BindingContainer
abstract class RuntimeBindings {
    @Multibinds(allowEmpty = true)
    abstract fun startables(): Set<AppStartable>

    companion object {
        @Provides
        @SingleIn(AppScope::class)
        fun provideAppScope(): CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

        @Provides
        @IoDispatcher
        fun provideIoDispatcher(): CoroutineDispatcher = Dispatchers.IO

        @Provides
        @MainDispatcher
        fun provideMainDispatcher(): CoroutineDispatcher = Dispatchers.Main.immediate

        @Provides
        @SingleIn(AppScope::class)
        fun provideClock(): Clock = AndroidClock

        @Provides
        @SingleIn(AppScope::class)
        fun provideServerClock(clock: Clock): ServerClock = ServerClock(clock)
    }
}
