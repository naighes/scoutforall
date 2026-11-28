package io.github.naighes.scout4all

import android.content.Intent
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import io.github.naighes.scout4all.core.PlayerForm
import io.github.naighes.scout4all.core.RoleEnum
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf

@RunWith(RobolectricTestRunner::class)
class PlayerProfileTest {
    @get:Rule val rule = createComposeRule()

    private val team = saveTeam("Italia")
    private var deleted = 0

    private fun egonu() = core {
        it.savePlayer(team, null, PlayerForm("Paola Egonu", RoleEnum.OPPOSITE_HITTER, "7", heightCm = "193"))
    }

    private fun show(player: String) {
        rule.setContent {
            Scout4AllTheme {
                PlayerProfileScreen(team, player, onBack = {}, onEdit = {}, onTeam = {}, onDeleted = { deleted++ })
            }
        }
    }

    @Test
    fun theProfileShowsTheNumberTheNameAndWhatIsKnown() {
        show(egonu())
        rule.waitForText("07")
        rule.onNodeWithText("PAOLA EGONU").assertExists()
        rule.onNodeWithText(string(R.string.no_matches_played)).assertExists()
        rule.onNodeWithText("193 ${string(R.string.centimetres)}").assertExists()
        // nothing written, nothing shown
        rule.onNodeWithText(string(R.string.federation_id).uppercase()).assertDoesNotExist()
    }

    @Test
    fun theFaceTouchesTheRightEdgeOfTheBand() {
        show(egonu())
        rule.waitForText("07")
        val band = rule.onNodeWithTag(PROFILE_BAND).getUnclippedBoundsInRoot()
        val face = rule.onNodeWithTag(PROFILE_FACE).getUnclippedBoundsInRoot()
        assertEquals(band.right, face.right)
        assertEquals(band.bottom, face.bottom)
    }

    @Test
    fun thePersonIsDeletedFromTheRegistryToo() {
        show(egonu())
        rule.waitForText("07")
        rule.onNodeWithContentDescription(string(R.string.remove_player)).performClick()
        rule.onNodeWithText(string(R.string.delete_person)).performClick()
        rule.waitFor { deleted == 1 }
        assertEquals(0, core { it.team(team) }.players.size)
        assertEquals(0, core { it.existingPlayers(saveTeam("Altra")) }.size)
    }

    @Test
    fun thePlayerIsReachedFromTheProfile() {
        val player = core {
            it.savePlayer(
                team,
                null,
                PlayerForm(
                    "Paola Egonu", RoleEnum.OPPOSITE_HITTER, "7",
                    email = "paola@example.com", phoneCountry = "IT", phone = "333 123 4567",
                    birthPlace = "Cittadella",
                ),
            )
        }
        show(player)
        rule.waitForText("Cittadella")
        // each button with its icon: mail, handset, WhatsApp
        rule.onAllNodesWithTag("contact-icon", useUnmergedTree = true).assertCountEquals(3)
        val opened = { button: String ->
            rule.onNodeWithText(button).performScrollTo().performClick()
            shadowOf(app).nextStartedActivity.let { it.action to it.dataString }
        }
        assertEquals(Intent.ACTION_SENDTO to "mailto:paola@example.com", opened(string(R.string.write_email)))
        assertEquals(Intent.ACTION_DIAL to "tel:+393331234567", opened(string(R.string.call)))
        assertEquals(Intent.ACTION_VIEW to "https://wa.me/393331234567", opened("WhatsApp"))
    }

    @Test
    fun aPlayerGoneIsToldNotThrown() {
        val player = egonu()
        core { it.removePlayer(team, player) }
        show(player)
        rule.waitForText(string(R.string.no_players))
        // as the other failed loads: a banner, not text alone
        rule.onNodeWithTag(BANNER_TAG, useUnmergedTree = true).assertExists()
    }

    @Test
    fun aRowOfTheTeamHasTheInitialsAndTheNumberAndOpensThePlayer() {
        val player = egonu()
        var opened: String? = null
        rule.setContent {
            Scout4AllTheme {
                TeamScreen(team, onBack = {}, onEdit = {}, onNewPlayer = {}, onPlayer = { opened = it }, onNewMatch = {}, onMatch = {})
            }
        }
        rule.waitForText("Paola Egonu")
        rule.onNodeWithText("PE", useUnmergedTree = true).assertExists()
        rule.onNodeWithText("7", useUnmergedTree = true).assertExists()
        rule.onNodeWithText("Paola Egonu").performClick()
        assertEquals(player, opened)
    }

    @Test
    fun theFieldsOfTheFormAreInTheOrderOfTheDesktop() {
        rule.setContent { Scout4AllTheme { PlayerFormScreen(team, null, onBack = {}, onDone = {}) } }
        rule.waitForText(string(R.string.number))
        val tops = listOf(
            R.string.name,
            R.string.number,
            R.string.role,
            R.string.birth_date,
            R.string.nationality,
            R.string.height,
            R.string.dominant_hand,
            R.string.federation_id,
            R.string.notes,
        ).map { rule.onNodeWithText(string(it)).getUnclippedBoundsInRoot().top }
        assertEquals(tops.sorted(), tops)
    }
}
