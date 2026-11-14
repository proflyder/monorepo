package dev.proflyder.home.feature.settings.presentation

import android.text.format.DateUtils

/** Relative "5 minutes ago" text in the device locale; no extra timestamp format. */
internal object HistoryFormat {
    fun relative(
        receivedAtMillis: Long,
        nowMillis: Long = System.currentTimeMillis(),
    ): CharSequence = DateUtils.getRelativeTimeSpanString(
        receivedAtMillis,
        nowMillis,
        DateUtils.MINUTE_IN_MILLIS,
        DateUtils.FORMAT_ABBREV_RELATIVE,
    )
}
