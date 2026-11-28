package io.github.naighes.scout4all

import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import io.github.naighes.scout4all.core.AgendaDay
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The week of the agendas is told by its days: "this week" was said of any
 * week the arrows had moved to. */
@RunWith(RobolectricTestRunner::class)
class AgendaWeekTest {
    @get:Rule val rule = createComposeRule()

    @Test
    fun theTitleTellsTheDaysShown() {
        val monday = testDay().let { it.minusDays((it.dayOfWeek.value - 1).toLong()) }.plusWeeks(2)
        val days = (0L..6L).map { AgendaDay(monday.plusDays(it).toString(), emptyList()) }
        rule.setContent {
            Scout4AllTheme {
                LazyColumn { agendaWeek(days, testDay(), false, {}, {}, {}, { _, _ -> }) }
            }
        }
        rule.onNodeWithText(weekRange(days.map { it.date })).assertExists()
    }
}
