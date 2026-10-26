package io.github.naighes.scout4all

import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.click
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** Going from screen to screen as the user does. */
@RunWith(RobolectricTestRunner::class)
class NavigationTest {
    @get:Rule val rule = createAndroidComposeRule<MainActivity>()

    /** Past the terms, to the home. */
    private fun start() {
        rule.waitForText(string(R.string.accept_terms))
        rule.onNode(hasText(string(R.string.accept_terms)) and hasClickAction()).performClick()
        rule.waitForText(string(R.string.app_name))
    }

    private fun back() = rule.onNode(hasContentDescription(string(R.string.back)) and hasClickAction())

    /** Two touches before the next frame: the screen is still there for the
     * second. */
    private fun SemanticsNodeInteraction.doubleTouch() = performTouchInput {
        click()
        advanceEventTime(100)
        click()
    }

    /** Two touches of the arrow back before the screen goes: once back, not
     * out of the teams into an empty screen. */
    @Test
    fun theArrowBackTouchedTwiceGoesBackOnce() {
        start()
        rule.onNode(hasContentDescription(string(R.string.settings)) and hasClickAction()).performClick()
        rule.waitForText(string(R.string.terms_of_use))
        back().doubleTouch()
        rule.waitForText(string(R.string.no_teams))
    }

    @Test
    fun theArrowBackOfATeamTouchedTwiceGoesBackOnce() {
        saveNationalTeam()
        start()
        rule.waitForText("Italia")
        rule.onNode(hasText("Italia") and hasClickAction()).performClick()
        rule.waitForText(nationalTeam.first().first, substring = true)
        back().doubleTouch()
        rule.waitForText("Italia")
    }
}
