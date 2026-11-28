package io.github.naighes.scout4all

import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewmodel.compose.LocalViewModelStoreOwner
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** A choice that doesn't go, while scouting: a banner, never a dialog. */
@RunWith(RobolectricTestRunner::class)
class ScoutingErrorTest {
    @get:Rule val rule = createComposeRule()

    private val team = saveNationalTeam()
    private val match = startFirstSet(team)
    private lateinit var model: ScoutingViewModel

    private fun banners() = rule.onAllNodesWithTag(BANNER_TAG, useUnmergedTree = true)

    private fun shown(count: Int) = rule.waitFor { banners().fetchSemanticsNodes().size == count }

    /** The side swapped where one only is scouted: refused by the core. */
    private fun refused() {
        rule.runOnUiThread { model.swapSide() }
        shown(1)
    }

    @Test
    fun anErrorStaysInABannerUntilTheNextChoiceOrTouchedAway() {
        rule.setContent {
            Scout4AllTheme {
                ScoutingScreen(team, match, onBack = {})
                model = ViewModelProvider(checkNotNull(LocalViewModelStoreOwner.current))[ScoutingViewModel::class.java]
            }
        }
        rule.waitForText("Cortona", substring = true)
        refused()
        rule.onAllNodesWithTag(NOTICE_TAG).assertCountEquals(0)
        // no time limit
        rule.mainClock.advanceTimeBy(30_000)
        banners().assertCountEquals(1)
        rule.onNodeWithTag(BANNER_TAG, useUnmergedTree = true).performClick()
        shown(0)
        refused()
        // the next choice that goes
        rule.runOnUiThread { model.invertedP1(false) }
        shown(0)
    }
}
