package dev.proflyder.home.feature.call

import dev.proflyder.home.core.runtime.AppStartable
import dev.proflyder.home.core.ui.ViewModelFactoryContributor
import dev.proflyder.home.feature.call.audio.AndroidCallAudioController
import dev.proflyder.home.feature.call.audio.CallAudioController
import dev.proflyder.home.feature.call.audio.CallAudioSession
import dev.proflyder.home.feature.call.media.WebRtcCallMediaFactory
import dev.proflyder.home.feature.call.presentation.AndroidCallPermissions
import dev.proflyder.home.feature.call.presentation.CallPermissions
import dev.proflyder.home.feature.call.presentation.CallViewModelFactoryContributor
import dev.proflyder.home.feature.call.session.CallManager
import dev.proflyder.home.feature.call.session.CallMediaFactory
import dev.proflyder.home.feature.call.session.CallSessionSettings
import dev.proflyder.home.feature.call.telecom.AndroidCallCoordinator
import dev.proflyder.home.feature.call.telecom.CallCoordinator
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.Binds
import dev.zacsweers.metro.BindingContainer
import dev.zacsweers.metro.ContributesTo
import dev.zacsweers.metro.IntoSet
import dev.zacsweers.metro.Provides

@ContributesTo(AppScope::class)
@BindingContainer
abstract class CallBindings {
    @Binds
    internal abstract fun controller(impl: CallManager): CallController

    @Binds
    internal abstract fun launcher(impl: AndroidCallLauncher): CallLauncher

    @Binds
    internal abstract fun mediaFactory(impl: WebRtcCallMediaFactory): CallMediaFactory

    @Binds
    internal abstract fun audio(impl: AndroidCallAudioController): CallAudioController

    @Binds
    internal abstract fun coordinator(impl: AndroidCallCoordinator): CallCoordinator

    @Binds
    internal abstract fun permissions(impl: AndroidCallPermissions): CallPermissions

    @Binds
    @IntoSet
    internal abstract fun audioSession(impl: CallAudioSession): AppStartable

    @Binds
    @IntoSet
    internal abstract fun foregroundStarter(impl: CallForegroundStarter): AppStartable

    @Binds
    @IntoSet
    internal abstract fun entryPointInstaller(impl: CallEntryPointInstaller): AppStartable

    @Binds
    @IntoSet
    internal abstract fun viewModel(impl: CallViewModelFactoryContributor): ViewModelFactoryContributor

    internal companion object {
        @Provides
        internal fun sessionSettings(): CallSessionSettings = CallSessionSettings()
    }
}
