package io.github.naighes.scout4all

import io.github.naighes.scout4all.core.MobileException
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.produceState
import androidx.compose.ui.unit.dp
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.DateRange
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SelectableDates
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.TimePicker
import androidx.compose.material3.rememberTimePickerState
import java.time.LocalTime
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.util.Locale

/** How a day is written in the fields: in the language of the phone, the
 * same everywhere ("14 novembre 2026"). */
internal fun dayText(day: LocalDate): String = day.format(DateTimeFormatter.ofLocalizedDate(FormatStyle.LONG))

/** The window of a cycle (days as saved, the last included) written as
 * the sessions write their days; one day alone. */
internal fun windowText(first: String, last: String): String {
    val written = DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM)
    val days = runCatching { LocalDate.parse(first).format(written) to LocalDate.parse(last).format(written) }
        .getOrDefault(first to last)
    return if (first == last) days.first else "${days.first} – ${days.second}"
}

/** Day and month in digits, in the order of the language. */
internal fun dayMonth(): DateTimeFormatter {
    val locale = Locale.getDefault()
    return DateTimeFormatter.ofPattern(android.text.format.DateFormat.getBestDateTimePattern(locale, "Md"), locale)
}

/** The days of a week from the first to the last ("11/23 – 11/29"), day
 * and month in the order of the language; nothing until they are read. */
internal fun weekRange(days: List<String>): String {
    if (days.isEmpty()) return ""
    val written = dayMonth()
    return listOf(days.first(), days.last()).joinToString(" – ") { LocalDate.parse(it).format(written) }
}

/** The field of a day, the same in every form (the match, the birth of a
 * player, a match imported without its date): the day written in the
 * language of the phone, a tap opens the calendar. Configured by the use:
 * `clear` (its description) lets the day be taken away; `latest` is the
 * last day to choose; `opensAt` the month shown without a day; `firstYear`
 * the first year of the calendar. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun DayField(
    label: String,
    day: LocalDate?,
    onDay: (LocalDate?) -> Unit,
    modifier: Modifier = Modifier.fillMaxWidth(),
    /** written instead when there's no day (a date the core can't read) */
    otherwise: String = "",
    error: String? = null,
    clear: String? = null,
    latest: LocalDate? = null,
    opensAt: LocalDate? = null,
    firstYear: Int? = null,
    /** the days already taken, listed under the calendar */
    busy: Set<LocalDate> = emptySet(),
) {
    var picking by rememberSaveable { mutableStateOf(false) }
    // a read-only field doesn't take clicks: its presses do
    val presses = remember { MutableInteractionSource() }
    LaunchedEffect(presses) {
        presses.interactions.collect { if (it is PressInteraction.Release) picking = true }
    }
    OutlinedTextField(
        value = day?.let(::dayText) ?: otherwise,
        onValueChange = {},
        modifier = modifier,
        readOnly = true,
        label = { Text(label) },
        trailingIcon = {
            when {
                day != null && clear != null -> IconButton(onClick = { onDay(null) }) {
                    Icon(Icons.Filled.Clear, contentDescription = clear)
                }
                else -> Icon(Icons.Filled.DateRange, contentDescription = null)
            }
        },
        singleLine = true,
        isError = error != null,
        supportingText = noteOf(error),
        interactionSource = presses,
    )
    if (picking) {
        // the picker counts days in UTC
        val millis = { d: LocalDate -> d.atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli() }
        val today = clock().today()
        val picker = rememberDatePickerState(
            initialSelectedDateMillis = day?.let(millis),
            initialDisplayedMonthMillis = millis(day ?: opensAt ?: today),
            yearRange = (firstYear ?: 1900)..((latest ?: today.plusYears(10)).year),
            selectableDates = object : SelectableDates {
                override fun isSelectableDate(utcTimeMillis: Long) =
                    latest?.let { utcTimeMillis <= millis(it) } ?: true
            },
        )
        DatePickerDialog(
            onDismissRequest = { picking = false },
            confirmButton = {
                TextButton(onClick = {
                    picker.selectedDateMillis?.let { selected ->
                        onDay(Instant.ofEpochMilli(selected).atZone(ZoneOffset.UTC).toLocalDate())
                    }
                    picking = false
                }) { Text(stringResource(R.string.ok)) }
            },
            dismissButton = {
                TextButton(onClick = { picking = false }) { Text(stringResource(R.string.cancel)) }
            },
        ) {
            Column {
                DatePicker(picker)
                // the picker draws no marks: the days of the month shown
                // that hold something are written under it
                val shown = picker.displayedMonthMillis.let { Instant.ofEpochMilli(it).atZone(ZoneOffset.UTC).toLocalDate() }
                val taken = busy.filter { it.year == shown.year && it.month == shown.month }.map { it.dayOfMonth }.sorted()
                if (taken.isNotEmpty()) {
                    Text(
                        stringResource(R.string.busy_days, taken.joinToString(", ")),
                        Modifier.padding(horizontal = 24.dp, vertical = 8.dp),
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
            }
        }
    }
}

/** The days of the team with a session or a match: for the calendars of
 * the forms to say which are taken. */
@Composable
internal fun rememberBusyDays(teamId: String): Set<LocalDate> {
    val app = androidx.compose.ui.platform.LocalContext.current.applicationContext as Scout4AllApplication
    val days by produceState(emptySet<LocalDate>(), teamId) {
        value = try {
            app.core { it.trainingAgendaBetween(teamId, null, null) }
                .mapNotNull { runCatching { LocalDate.parse(it.date.take(10)) }.getOrNull() }
                .toSet()
        } catch (_: MobileException) {
            emptySet()
        }
    }
    return days
}

/** The field of a time of day, as `DayField` is of a day: written as the
 * phone writes it, a tap opens the clock. `clear` (its description) lets
 * the time be taken away. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun TimeField(
    label: String,
    time: LocalTime?,
    onTime: (LocalTime?) -> Unit,
    modifier: Modifier = Modifier.fillMaxWidth(),
    error: String? = null,
    clear: String? = null,
) {
    var picking by rememberSaveable { mutableStateOf(false) }
    val presses = remember { MutableInteractionSource() }
    LaunchedEffect(presses) {
        presses.interactions.collect { if (it is PressInteraction.Release) picking = true }
    }
    OutlinedTextField(
        value = time?.format(DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT)).orEmpty(),
        onValueChange = {},
        modifier = modifier,
        readOnly = true,
        label = { Text(label) },
        trailingIcon = {
            when {
                time != null && clear != null -> IconButton(onClick = { onTime(null) }) {
                    Icon(Icons.Filled.Clear, contentDescription = clear)
                }
                else -> Icon(Icons.Filled.DateRange, contentDescription = null)
            }
        },
        singleLine = true,
        isError = error != null,
        supportingText = noteOf(error),
        interactionSource = presses,
    )
    if (picking) {
        val at = time ?: LocalTime.of(18, 0)
        val picker = rememberTimePickerState(
            initialHour = at.hour,
            initialMinute = at.minute,
            is24Hour = android.text.format.DateFormat.is24HourFormat(androidx.compose.ui.platform.LocalContext.current),
        )
        AlertDialog(
            onDismissRequest = { picking = false },
            text = { TimePicker(picker) },
            confirmButton = {
                TextButton(onClick = {
                    onTime(LocalTime.of(picker.hour, picker.minute))
                    picking = false
                }) { Text(stringResource(R.string.ok)) }
            },
            dismissButton = {
                TextButton(onClick = { picking = false }) { Text(stringResource(R.string.cancel)) }
            },
        )
    }
}
