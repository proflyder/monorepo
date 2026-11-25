package dev.proflyder.home.core.launchericon

import dev.proflyder.home.core.runtime.AppForeground
import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.core.runtime.IoDispatcher
import dev.proflyder.home.core.theme.AppTheme
import dev.proflyder.home.core.theme.AppearanceController
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

@ContributesIntoSet(AppScope::class)
@Inject
class LauncherIconSync(
    private val appearance: AppearanceController,
    private val icons: LauncherIcons,
    private val foreground: AppForeground,
    private val scope: CoroutineScope,
    @IoDispatcher private val io: CoroutineDispatcher,
) : AppStartable {
    override fun start() {
        scope.launch(io) { themesOnBackground().filter { it != icons.shown() }.collect(icons::show) }
    }

    private fun themesOnBackground(): Flow<AppTheme> = flow {
        appearance.loaded.first { it }
        emitAll(backgroundings().map { appearance.state.value.theme })
    }

    private fun backgroundings(): Flow<Boolean> = foreground.state
        .map { it.foreground }
        .distinctUntilChanged()
        .drop(1)
        .filter { !it }
}
