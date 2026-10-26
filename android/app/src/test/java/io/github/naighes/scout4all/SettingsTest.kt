package io.github.naighes.scout4all

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class SettingsTest {
    @get:Rule val rule = createComposeRule()

    @Test
    fun theStatisticsAreSentUntilTurnedOff() {
        val opened = mutableListOf<String>()
        rule.setContent {
            Scout4AllTheme {
                SettingsScreen(onBack = {}, onTerms = { opened += "terms" }, onAbout = { opened += "about" })
            }
        }
        rule.waitForText(string(R.string.send_statistics))
        assertEquals(true, core { it.analyticsEnabled() })
        rule.onNode(hasText(string(R.string.send_statistics)) and hasClickAction()).performClick()
        rule.waitFor { !core { it.analyticsEnabled() } }
        rule.onNode(hasText(string(R.string.send_statistics)) and hasClickAction()).performClick()
        rule.waitFor { core { it.analyticsEnabled() } }
        rule.onNode(hasText(string(R.string.terms_of_use)) and hasClickAction()).performClick()
        rule.onNode(hasText(string(R.string.about)) and hasClickAction()).performClick()
        assertEquals(listOf("terms", "about"), opened)
    }
}

/** The terms at the first start, then the teams; not again. */
@RunWith(RobolectricTestRunner::class)
class FirstStartTest {
    @get:Rule val rule = createAndroidComposeRule<MainActivity>()

    @Test
    fun theTermsThenTheTeams() {
        rule.waitForText(string(R.string.accept_terms))
        assertEquals(false, core { it.termsSeen() })
        rule.onNode(hasText(string(R.string.accept_terms)) and hasClickAction()).performClick()
        rule.waitForText(string(R.string.no_teams))
        assertEquals(true, core { it.termsSeen() })
        // started again: straight to the teams
        rule.activityRule.scenario.recreate()
        rule.waitForText(string(R.string.no_teams))
        assertEquals(0, rule.onAllNodes(hasText(string(R.string.accept_terms))).fetchSemanticsNodes().size)
    }
}
