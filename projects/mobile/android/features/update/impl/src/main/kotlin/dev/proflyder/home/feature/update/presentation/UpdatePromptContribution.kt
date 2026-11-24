package dev.proflyder.home.feature.update.presentation

import androidx.appcompat.app.AppCompatActivity
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import dev.proflyder.home.core.navigation.AppActivityContribution
import dev.proflyder.home.feature.update.UpdateCenter
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import kotlinx.coroutines.launch

@ContributesIntoSet(AppScope::class)
@Inject
class UpdatePromptContribution internal constructor(
    private val updates: UpdateCenter,
) : AppActivityContribution {
    override val order: Int = PROMPT_ORDER

    override fun attach(activity: AppCompatActivity) {
        if (!updates.supported) return
        val dialog = UpdatePromptDialog(activity, updates)
        activity.lifecycleScope.launch {
            activity.repeatOnLifecycle(Lifecycle.State.STARTED) {
                updates.state.collect(dialog::render)
            }
        }
        activity.lifecycle.addObserver(DismissOnDestroy(dialog))
    }

    private class DismissOnDestroy(private val dialog: UpdatePromptDialog) : DefaultLifecycleObserver {
        override fun onDestroy(owner: LifecycleOwner) = dialog.dismiss()
    }

    private companion object {
        const val PROMPT_ORDER = 100
    }
}
