package dev.proflyder.home.core.theme

import android.app.Activity
import android.app.Application
import android.os.Build
import android.os.Bundle
import android.view.ViewTreeObserver
import androidx.appcompat.app.AppCompatDelegate
import java.util.WeakHashMap
import kotlinx.coroutines.flow.StateFlow

internal class ThemeApplier(
    private val current: () -> AppTheme,
    private val ready: StateFlow<Boolean>,
) : Application.ActivityLifecycleCallbacks {
    private val applied = WeakHashMap<Activity, AppTheme>()

    fun onAppearance(appearance: Appearance) {
        AppCompatDelegate.setDefaultNightMode(nightModeOf(appearance.mode))
        applied.filterValues { it != appearance.theme }.keys.toList().forEach(Activity::recreate)
    }

    override fun onActivityPreCreated(activity: Activity, savedInstanceState: Bundle?) = apply(activity)

    override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) apply(activity)
    }

    override fun onActivityStarted(activity: Activity) {
        if (!ready.value) holdFirstFrame(activity)
    }

    override fun onActivityResumed(activity: Activity) = Unit

    override fun onActivityPaused(activity: Activity) = Unit

    override fun onActivityStopped(activity: Activity) = Unit

    override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) = Unit

    override fun onActivityDestroyed(activity: Activity) {
        applied.remove(activity)
    }

    private fun apply(activity: Activity) {
        val theme = current()
        AppThemes.applyTo(activity, theme)
        applied[activity] = theme
    }

    private fun holdFirstFrame(activity: Activity) {
        val observer = activity.window.decorView.viewTreeObserver
        observer.addOnPreDrawListener(FirstFrameGate(observer, ready))
    }
}

private class FirstFrameGate(
    private val observer: ViewTreeObserver,
    private val ready: StateFlow<Boolean>,
) : ViewTreeObserver.OnPreDrawListener {
    override fun onPreDraw(): Boolean {
        val open = ready.value
        if (open && observer.isAlive) observer.removeOnPreDrawListener(this)
        return open
    }
}
