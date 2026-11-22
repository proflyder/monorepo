package dev.proflyder.home.core.runtime

import dev.proflyder.home.core.model.AnalyticsBackend
import dev.proflyder.home.core.model.AnalyticsTracker
import dev.proflyder.home.core.model.AppEnvironment
import dev.proflyder.home.core.model.AppLogger
import dev.proflyder.home.core.model.Clock
import dev.proflyder.home.core.model.CompositeAnalyticsTracker
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

    @Multibinds(allowEmpty = true)
    abstract fun analyticsBackends(): Set<AnalyticsBackend>

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

        @Provides
        @SingleIn(AppScope::class)
        fun provideEnvironment(version: AppVersion): AppEnvironment =
            if (version.debuggable) AppEnvironment.DEBUG else AppEnvironment.RELEASE

        @Provides
        @SingleIn(AppScope::class)
        fun provideLogger(environment: AppEnvironment): AppLogger = when (environment) {
            AppEnvironment.DEBUG -> DebugAppLogger()
            AppEnvironment.RELEASE -> ReleaseAppLogger()
        }

        @Provides
        @SingleIn(AppScope::class)
        fun provideAnalytics(backends: Set<AnalyticsBackend>, environment: AppEnvironment): AnalyticsTracker =
            CompositeAnalyticsTracker(backends, environment)
    }
}
