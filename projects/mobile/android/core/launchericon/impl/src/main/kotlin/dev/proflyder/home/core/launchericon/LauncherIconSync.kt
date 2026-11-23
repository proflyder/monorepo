package dev.proflyder.home.core.launchericon

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
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

@ContributesIntoSet(AppScope::class)
@Inject
class LauncherIconSync(
    private val appearance: AppearanceController,
    private val icons: LauncherIcons,
    private val scope: CoroutineScope,
    @IoDispatcher private val io: CoroutineDispatcher,
) : AppStartable {
    override fun start() {
        scope.launch(io) { storedThemes().collect(icons::show) }
    }

    private fun storedThemes(): Flow<AppTheme> = flow {
        appearance.loaded.first { it }
        emitAll(appearance.state.map { it.theme }.distinctUntilChanged())
    }
}
