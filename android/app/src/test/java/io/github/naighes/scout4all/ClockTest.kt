package io.github.naighes.scout4all

import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import io.github.naighes.scout4all.core.PlayerForm
import io.github.naighes.scout4all.core.RoleEnum
import java.time.LocalDateTime
import java.time.ZoneOffset
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The app and the core ask the clock of the app what time it is: a
 * stopped one, years from now, and both tell it. */
@RunWith(RobolectricTestRunner::class)
class ClockTest {
    @get:Rule val rule = createComposeRule()

    private val stopped = FixedClock(LocalDateTime.of(2041, 3, 9, 0, 30), ZoneOffset.ofHours(1))

    @Before
    fun stop() {
        (app as TestApplication).clock = stopped
    }

    @Test
    fun aNewTeamStartsInTheYearOfTheClock() {
        rule.setContent { Scout4AllTheme { TeamFormScreen(null, onBack = {}, onSaved = {}) } }
        rule.waitForText(string(R.string.name))
        rule.onNode(hasSetTextAction() and hasText(string(R.string.year))).assertTextContains("2041")
    }

    @Test
    fun theCoreTellsTheAgeOnTheDayOfTheClock() {
        val team = saveTeam("Italia", 2040)
        val player = core { it.savePlayer(team, null, PlayerForm("Paola Egonu", RoleEnum.OPPOSITE_HITTER, "18", birthDate = "2001-03-10")) }
        // a day short of 40
        assertEquals(39u, core { it.playerProfile(team, player) }.age)
        (app as TestApplication).clock = FixedClock(LocalDateTime.of(2041, 3, 10, 0, 0), ZoneOffset.ofHours(1))
        assertEquals(40u, core { it.playerProfile(team, player) }.age)
    }
}
