package io.github.naighes.scout4all

import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class TeamsScreenTest {
    @get:Rule val rule = createComposeRule()

    private fun show(onTeam: (String) -> Unit = {}) = rule.setContent {
        Scout4AllTheme {
            TeamsScreen(TeamsViewModel(app), onTeam = onTeam, onNewTeam = {}, onSettings = {}, onMatch = { _, _ -> })
        }
    }

    @Test
    fun noTeams() {
        show()
        rule.waitForText(string(R.string.no_teams))
        rule.onNodeWithText(string(R.string.no_teams)).assertIsDisplayed()
    }

    @Test
    fun theTeamsWithTheirYearAndPlayers() {
        val cortona = saveTeam("Cortona")
        saveTeam("Arezzo", year = 2025)
        val opened = mutableListOf<String>()
        show { opened += it }
        rule.waitForText("Cortona")
        rule.onNodeWithText("Arezzo").assertIsDisplayed()
        rule.onNodeWithText("2026 · 0 players").assertIsDisplayed()
        rule.onAllNodesWithText(string(R.string.no_teams)).assertCountEquals(0)
        rule.onNodeWithText("Cortona").performClick()
        assertEquals(listOf(cortona), opened)
    }
}
