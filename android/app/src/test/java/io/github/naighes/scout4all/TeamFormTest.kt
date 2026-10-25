package io.github.naighes.scout4all

import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import io.github.naighes.scout4all.core.GenderEnum
import io.github.naighes.scout4all.core.TeamClassificationEnum
import io.github.naighes.scout4all.core.TeamListing
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class TeamFormTest {
    @get:Rule val rule = createComposeRule()

    private var saved = 0

    private fun show(id: String? = null) {
        rule.setContent { Scout4AllTheme { TeamFormScreen(id, onBack = {}, onSaved = { saved++ }) } }
        rule.waitForText(string(R.string.name))
    }

    private fun teams() = core { it.teams() }.filterIsInstance<TeamListing.Team>()

    @Test
    fun aTeamWithoutNameIsNotSaved() {
        show()
        rule.save()
        rule.assertInvalid(string(R.string.name))
        assertEquals(0, saved)
        assertEquals(emptyList<TeamListing.Team>(), teams())
    }

    @Test
    fun theErrorGoesWhenTheFieldChanges() {
        show()
        rule.save()
        rule.assertInvalid(string(R.string.name))
        rule.type(string(R.string.name), "Italia")
        rule.assertValid(string(R.string.name))
    }

    private fun choose(gender: GenderEnum, classification: TeamClassificationEnum) {
        rule.onNodeWithText(core { it.genders() }.first { it.value == gender }.name).performScrollTo().performClick()
        rule.onNodeWithText(core { it.classifications() }.first { it.value == classification }.name).performScrollTo().performClick()
    }

    @Test
    fun aYearThatIsNotOneIsRejected() {
        show()
        rule.type(string(R.string.name), "Italia")
        rule.type(string(R.string.year), "twenty")
        choose(GenderEnum.WOMEN, TeamClassificationEnum.REGIONAL)
        rule.save()
        rule.assertInvalid(string(R.string.year))
        rule.assertValid(string(R.string.name))
        assertEquals(0, saved)
    }

    @Test
    fun aNewTeam() {
        show()
        rule.type(string(R.string.name), "Italia")
        rule.type(string(R.string.year), "2026")
        choose(GenderEnum.WOMEN, TeamClassificationEnum.TOP_INTERNATIONAL)
        rule.save()
        rule.waitFor { saved == 1 }
        val team = teams().single()
        assertEquals("Italia" to 2026, team.name to team.year.toInt())
        val form = core { it.team(team.id) }.form
        assertEquals(GenderEnum.WOMEN to TeamClassificationEnum.TOP_INTERNATIONAL, form.gender to form.classification)
    }

    @Test
    fun aTeamEdited() {
        val id = saveTeam("Italia")
        show(id)
        rule.field(string(R.string.name)).assertTextContains("Italia")
        rule.type(string(R.string.name), "Italia U20")
        rule.save()
        rule.waitFor { saved == 1 }
        assertEquals(listOf("Italia U20"), teams().map { it.name })
        rule.onAllNodesWithText(string(R.string.new_team)).assertCountEquals(0)
    }
}
