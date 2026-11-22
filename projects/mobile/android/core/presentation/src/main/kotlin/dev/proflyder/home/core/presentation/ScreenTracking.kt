package dev.proflyder.home.core.presentation

import android.app.Activity
import android.app.Application
import android.content.Context
import android.os.Bundle
import androidx.fragment.app.Fragment
import androidx.fragment.app.FragmentActivity
import androidx.fragment.app.FragmentManager
import dev.proflyder.home.core.model.AnalyticsTracker
import dev.proflyder.home.core.runtime.AppStartable
import dev.zacsweers.metro.AppScope
import dev.zacsweers.metro.ContributesIntoSet
import dev.zacsweers.metro.Inject
import dev.zacsweers.metro.binding

interface TrackedScreen {
    val screenName: String
}

internal fun screenNameOf(screen: Any): String =
    screen.javaClass.simpleName.removeSuffix("Fragment").removeSuffix("Sheet").ifBlank { screen.javaClass.name }

class ScreenResumeTracker(private val tracker: AnalyticsTracker) : FragmentManager.FragmentLifecycleCallbacks() {
    override fun onFragmentResumed(fm: FragmentManager, f: Fragment) {
        (f as? TrackedScreen)?.let { tracker.trackScreen(it.screenName) }
    }
}

@ContributesIntoSet(AppScope::class, binding = binding<AppStartable>())
@Inject
class ScreenTracking(private val context: Context, tracker: AnalyticsTracker) : AppStartable {
    private val resumes = ScreenResumeTracker(tracker)

    override fun start() {
        val application = context.applicationContext as? Application ?: return
        application.registerActivityLifecycleCallbacks(ActivityCreated(::attach))
    }

    private fun attach(activity: Activity) {
        (activity as? FragmentActivity)?.supportFragmentManager?.registerFragmentLifecycleCallbacks(resumes, true)
    }
}

private class ActivityCreated(private val onCreated: (Activity) -> Unit) : Application.ActivityLifecycleCallbacks {
    override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) = onCreated(activity)

    override fun onActivityStarted(activity: Activity) = Unit

    override fun onActivityResumed(activity: Activity) = Unit

    override fun onActivityPaused(activity: Activity) = Unit

    override fun onActivityStopped(activity: Activity) = Unit

    override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) = Unit

    override fun onActivityDestroyed(activity: Activity) = Unit
}
