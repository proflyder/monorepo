package dev.proflyder.home.core.theme

import android.app.Activity
import android.app.Application
import android.os.Build
import android.os.Bundle

/**
 * Themes every activity before its first frame. On Android 10+ the overlay is
 * applied in `onActivityPreCreated`, before `Activity.onCreate`; older systems
 * get it in `onActivityCreated`, which still runs before `setContentView`.
 */
internal class ThemeApplier(private val current: () -> AppTheme) : Application.ActivityLifecycleCallbacks {
    override fun onActivityPreCreated(activity: Activity, savedInstanceState: Bundle?) {
        AppThemes.applyTo(activity, current())
    }

    override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) AppThemes.applyTo(activity, current())
    }

    override fun onActivityStarted(activity: Activity) = Unit

    override fun onActivityResumed(activity: Activity) = Unit

    override fun onActivityPaused(activity: Activity) = Unit

    override fun onActivityStopped(activity: Activity) = Unit

    override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) = Unit

    override fun onActivityDestroyed(activity: Activity) = Unit
}
