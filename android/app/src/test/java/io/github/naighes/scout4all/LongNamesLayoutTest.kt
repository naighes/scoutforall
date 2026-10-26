package io.github.naighes.scout4all

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.semantics.SemanticsActions
import io.github.naighes.scout4all.core.EvalEnum
import io.github.naighes.scout4all.core.MatchForm
import io.github.naighes.scout4all.core.PlayerForm
import io.github.naighes.scout4all.core.RoleEnum
import io.github.naighes.scout4all.core.TeamSideEnum
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner

/** The scouting of a team with long names, as the one of the user, fits
 * every screen: the team name in two lines, with the dots if it doesn't
 * fit (TalkBack reads it whole), no other text cut, never a scroll. Up to
 * the font 1.3: larger, the court would shrink to a stamp. */
@RunWith(ParameterizedRobolectricTestRunner::class)
class LongNamesLayoutTest(private val screen: Size) {
    @get:Rule(order = 0) val size = SizeRule(screen)
    @get:Rule(order = 1) val rule = createComposeRule()

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun sizes() = sizes.map { arrayOf(it) }

        private const val TEAM = "Cortona Volley (serie C)"
        private val players = listOf(
            Triple("Alessandro Brilli", RoleEnum.OUTSIDE_HITTER, 13),
            Triple("Alessandro Magrini", RoleEnum.SETTER, 12),
            Triple("Daniele Bettoni", RoleEnum.MIDDLE_BLOCKER, 17),
            Triple("Alessandro Mogini", RoleEnum.OUTSIDE_HITTER, 27),
            Triple("Christian Di Balsamo", RoleEnum.OPPOSITE_HITTER, 28),
            Triple("Alessandro Augero", RoleEnum.MIDDLE_BLOCKER, 6),
            Triple("Riccardo Bartolozzi", RoleEnum.LIBERO, 1),
        )
    }

    private fun scout(): String {
        val team = saveTeam(TEAM)
        players.forEach { (name, role, number) -> core { it.savePlayer(team, null, PlayerForm(name, role, number.toString())) } }
        val match = core { c ->
            val match = c.saveMatch(team, null, MatchForm("Remo Masi", "2026-10-25T18:00:00+02:00", true, "6"))
            val start = c.setStart(team, match)
            start.chooseServingTeam(TeamSideEnum.US)
            val ids = c.team(team).players.associate { it.name to it.id }
            players.take(6).forEachIndexed { i, (name, _, _) -> start.place((i + 1).toUByte(), ids.getValue(name)) }
            start.chooseSetter(ids.getValue("Alessandro Magrini"))
            start.chooseLibero(ids.getValue("Riccardo Bartolozzi"))
            c.startSet(start)
            match
        }
        serve(team, match, EvalEnum.POSITIVE)
        rule.setContent { Scout4AllTheme { ScoutingScreen(team, match, onBack = {}) } }
        rule.waitForText("Remo Masi", substring = true)
        return team
    }

    private fun assertFits(name: String) {
        rule.shot("long-names-$name-$screen")
        assertEquals(emptyList<String>(), rule.misfits(screen.scrolls).filterNot { it == "text cut: $TEAM" } + rule.inaccessible())
        val read = rule.onAllNodes(SemanticsMatcher("any") { true }).fetchSemanticsNodes()
            .mapNotNull { it.config.getOrNull(SemanticsProperties.Text)?.joinToString() }
        assertTrue("$read", read.any { it.startsWith("$TEAM 0, Remo Masi 0") })
    }

    @Test
    fun theirSideAfterTheServe() {
        scout()
        assertFits("their-side")
    }

    @Test
    fun theEvaluation() {
        scout()
        val player = hasText("28") and hasClickAction() and isEnabled()
        rule.waitFor { rule.onAllNodes(player).fetchSemanticsNodes().size == 1 }
        rule.onNode(player).performSemanticsAction(SemanticsActions.OnClick)
        // in the back row: only the defense, straight to its evaluation
        rule.waitForText("defense · 28", substring = true)
        assertFits("evaluation")
    }
}
