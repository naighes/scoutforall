package io.github.naighes.scout4all

import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
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

@RunWith(RobolectricTestRunner::class)
class PlayerFormTest {
    @get:Rule val rule = createComposeRule()

    private val team = saveTeam("Italia")
    private var done = 0

    private fun show(player: String? = null) {
        rule.setContent { Scout4AllTheme { PlayerFormScreen(team, player, onBack = {}, onDone = { done++ }) } }
        rule.waitForText(string(R.string.number))
    }

    private fun players() = core { it.team(team) }.players

    private fun role(role: RoleEnum) = core { it.roles() }.first { it.value == role }.name

    @Test
    fun aPlayerWithoutNumberIsNotSaved() {
        show()
        rule.type(string(R.string.name), "Paola Egonu")
        rule.onNodeWithText(role(RoleEnum.OPPOSITE_HITTER)).performScrollTo().performClick()
        rule.save()
        rule.assertInvalid(string(R.string.number))
        assertEquals(0, done)
        assertEquals(0, players().size)
    }

    @Test
    fun aNumberTakenIsRejected() {
        core { it.savePlayer(team, null, PlayerForm("Alessia Orro", RoleEnum.SETTER, "8")) }
        show()
        rule.type(string(R.string.name), "Paola Egonu")
        rule.type(string(R.string.number), "8")
        rule.onNodeWithText(role(RoleEnum.OPPOSITE_HITTER)).performScrollTo().performClick()
        rule.save()
        rule.assertInvalid(string(R.string.number))
        assertEquals(listOf("Alessia Orro"), players().map { it.name })
    }

    @Test
    fun aNewPlayer() {
        show()
        rule.type(string(R.string.name), "Paola Egonu")
        rule.type(string(R.string.number), "18")
        rule.onNodeWithText(role(RoleEnum.OPPOSITE_HITTER)).performScrollTo().performClick()
        rule.save()
        rule.waitFor { done == 1 }
        val player = players().single()
        assertEquals(Triple("Paola Egonu", 18, role(RoleEnum.OPPOSITE_HITTER)), Triple(player.name, player.number.toInt(), player.role))
    }

    @Test
    fun aPlayerEdited() {
        val id = core { it.savePlayer(team, null, PlayerForm("Paola Egonu", RoleEnum.OPPOSITE_HITTER, "18")) }
        show(id)
        rule.field(string(R.string.number)).assertTextContains("18")
        rule.type(string(R.string.number), "81")
        rule.save()
        rule.waitFor { done == 1 }
        assertEquals(listOf(81), players().map { it.number.toInt() })
    }

    @Test
    fun aPlayerRemovedAfterAsking() {
        val id = core { it.savePlayer(team, null, PlayerForm("Paola Egonu", RoleEnum.OPPOSITE_HITTER, "18")) }
        show(id)
        rule.onNodeWithContentDescription(string(R.string.remove_player)).performClick()
        rule.onNodeWithText(string(R.string.cancel)).performClick()
        assertEquals(1, players().size)
        rule.onNodeWithContentDescription(string(R.string.remove_player)).performClick()
        rule.onNodeWithText(string(R.string.remove)).performClick()
        rule.waitFor { done == 1 }
        assertEquals(0, players().size)
    }
}
