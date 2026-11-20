package dev.proflyder.home.feature.settings.presentation

import android.text.format.DateUtils

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
