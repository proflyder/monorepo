package dev.proflyder.home.feature.alarms.presentation

import android.content.Context
import dev.proflyder.home.feature.alarms.impl.R
import dev.zacsweers.metro.Inject

@Inject
internal class DefaultAlarmLabel(private val context: Context) {
    fun value(): String = context.getString(R.string.alarms_label_default)
}
