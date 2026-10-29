package dev.proflyder.home.core.ui

import androidx.fragment.app.FragmentManager
import com.google.android.material.datepicker.CalendarConstraints
import com.google.android.material.datepicker.DateValidatorPointForward
import com.google.android.material.datepicker.MaterialDatePicker
import com.google.android.material.timepicker.MaterialTimePicker
import com.google.android.material.timepicker.TimeFormat
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneOffset

/** Material 24-hour time picker. */
fun FragmentManager.showTimePicker(hour: Int, minute: Int, onPicked: (hour: Int, minute: Int) -> Unit) {
    val picker = MaterialTimePicker.Builder()
        .setTimeFormat(TimeFormat.CLOCK_24H)
        .setHour(hour)
        .setMinute(minute)
        .build()
    picker.addOnPositiveButtonClickListener { onPicked(picker.hour, picker.minute) }
    picker.show(this, "time_picker")
}

/**
 * Material date picker. Material dates are UTC midnights, so conversion goes
 * through [ZoneOffset.UTC]; [notBefore] disables earlier days.
 */
fun FragmentManager.showDatePicker(selected: LocalDate, notBefore: LocalDate, onPicked: (LocalDate) -> Unit) {
    val picker = MaterialDatePicker.Builder.datePicker()
        .setSelection(selected.atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli())
        .setCalendarConstraints(
            CalendarConstraints.Builder()
                .setStart(notBefore.atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli())
                .setValidator(
                    DateValidatorPointForward.from(notBefore.atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli()),
                )
                .build(),
        )
        .build()
    picker.addOnPositiveButtonClickListener { millis ->
        onPicked(Instant.ofEpochMilli(millis).atZone(ZoneOffset.UTC).toLocalDate())
    }
    picker.show(this, "date_picker")
}
