package io.github.naighes.scout4all

import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.foundation.layout.fillMaxWidth
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

/** How a day is written in the fields: in the language of the phone, the
 * same everywhere ("14 novembre 2026"). */
internal fun dayText(day: LocalDate): String = day.format(DateTimeFormatter.ofLocalizedDate(FormatStyle.LONG))

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
        supportingText = error?.let { { Text(it) } },
        interactionSource = presses,
    )
    if (picking) {
        // the picker counts days in UTC
        val millis = { d: LocalDate -> d.atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli() }
        val today = LocalDate.now()
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
        ) { DatePicker(picker) }
    }
}
