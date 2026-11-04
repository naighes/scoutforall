package io.github.naighes.scout4all

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.performSemanticsAction
import io.github.naighes.scout4all.core.EvalEnum
import io.github.naighes.scout4all.core.EventTypeEnum
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner

/** The statistics of a match fit every screen: they may scroll, no text
 * cut. */
@RunWith(ParameterizedRobolectricTestRunner::class)
class StatsLayoutTest(private val screen: Size) {
    @get:Rule(order = 0) val size = SizeRule(screen)
    @get:Rule(order = 1) val rule = createComposeRule()

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun sizes() = sizes.map { arrayOf(it) }
    }

    @Test
    fun theStatisticsOfTheMatch() {
        val team = saveNationalTeam()
        val match = startFirstSet(team)
        // the serves by zone in the detail: to 4/3 (3D and 4A), to 1, then
        // without one
        core { c ->
            c.scouting(team, match).use { s ->
                s.touchTarget(3u, "D")
                s.touchTarget(4u, "A")
                s.chooseEval(EvalEnum.PERFECT)
                s.touchTarget(1u, "A")
                s.chooseEval(EvalEnum.PERFECT)
                // their serve received on 6/1 (1D and 6A), our attack to 5A
                s.chooseEval(EvalEnum.ERROR)
                s.chooseEvent(EventTypeEnum.P)
                s.choosePlayer(s.playerChoices().first().id)
                s.touchTarget(1u, "D")
                s.touchTarget(6u, "A")
                s.chooseEval(EvalEnum.POSITIVE)
                s.chooseEvent(EventTypeEnum.A)
                s.choosePlayer(s.playerChoices().first().id)
                s.touchTarget(5u, "A")
                s.chooseEval(EvalEnum.PERFECT)
                // our serve, their attack dug on 6/1 (1C and 6B), our kill
                s.chooseEval(EvalEnum.POSITIVE)
                s.chooseEvent(EventTypeEnum.D)
                s.choosePlayer(s.playerChoices().first().id)
                s.touchTarget(1u, "C")
                s.touchTarget(6u, "B")
                s.chooseEval(EvalEnum.POSITIVE)
                s.chooseEvent(EventTypeEnum.A)
                s.choosePlayer(s.playerChoices().first().id)
                s.chooseEval(EvalEnum.PERFECT)
                // our serve, their attack blocked in 3, our kill
                s.chooseEval(EvalEnum.POSITIVE)
                s.chooseEvent(EventTypeEnum.B)
                s.choosePlayer(s.playerChoices().first().id)
                s.touchTarget(3u, "")
                s.chooseEval(EvalEnum.POSITIVE)
                s.chooseEvent(EventTypeEnum.A)
                s.choosePlayer(s.playerChoices().first().id)
                s.chooseEval(EvalEnum.PERFECT)
            }
        }
        serve(team, match, EvalEnum.PERFECT, EvalEnum.POSITIVE)
        rule.setContent { Scout4AllTheme { MatchStatsScreen(team, match, null, onBack = {}) } }
        rule.waitForText(string(R.string.stats_summary))
        val pages = listOf(R.string.stats_summary, R.string.stats_distribution, R.string.stats_errors, R.string.stats_trend, R.string.stats_detail)
        val misfits = pages.associate { page ->
            // the tab may be out of its row
            rule.onNode(hasText(string(page)) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
            rule.waitForIdle()
            rule.shot("stats-${string(page)}-$screen")
            string(page) to rule.misfits(scrolls = true) + rule.inaccessible()
        }
        assertEquals(pages.associate { string(it) to emptyList<String>() }, misfits)
        // the detail of the serve: the zones, then their efficiency
        rule.waitForText(string(R.string.serves_by_zone))
        val efficiency = string(R.string.efficiency).replaceFirstChar { it.uppercase() }
        rule.onNode(hasText(efficiency) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForIdle()
        rule.shot("stats-zone-efficiency-$screen")
        assertEquals(emptyList<String>(), rule.misfits(scrolls = true) + rule.inaccessible())
        // the reception and the defense, with the conflict zones; the
        // block, at the net; the attack, by zone
        rule.onNode(hasText("Reception") and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForText(string(R.string.receptions_by_zone))
        rule.waitForText(string(R.string.on_conflicts, 1, 100), substring = true)
        rule.shot("stats-reception-zones-$screen")
        assertEquals(emptyList<String>(), rule.misfits(scrolls = true) + rule.inaccessible())
        rule.onNode(hasText("Defense") and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForText(string(R.string.digs_by_zone))
        rule.waitForText(string(R.string.on_conflicts, 1, 100), substring = true)
        rule.shot("stats-dig-zones-$screen")
        assertEquals(emptyList<String>(), rule.misfits(scrolls = true) + rule.inaccessible())
        rule.onNode(hasText("Block") and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForText(string(R.string.blocks_by_zone))
        rule.shot("stats-block-zones-$screen")
        assertEquals(emptyList<String>(), rule.misfits(scrolls = true) + rule.inaccessible())
        rule.onNode(hasText("Attack") and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForText(string(R.string.attacks_by_zone))
        rule.shot("stats-attack-zones-$screen")
        assertEquals(emptyList<String>(), rule.misfits(scrolls = true) + rule.inaccessible())
    }
}

/** The attacks of each zone counted in the language of the app. */
@RunWith(ParameterizedRobolectricTestRunner::class)
class ZoneAttacksLanguageTest(private val language: String, private val count: String) {
    @get:Rule(order = 0) val size = SizeRule(Size(language, "$language-w392dp-h800dp-port"))
    @get:Rule(order = 1) val rule = createComposeRule()

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun languages() = listOf(arrayOf("pt", "1 atq"), arrayOf("ja", "1本"))
    }

    @Test
    fun theAttacksOfAZone() {
        val team = saveNationalTeam()
        val match = startFirstSet(team)
        // their reception straight back, then Egonu's kill
        serve(team, match, EvalEnum.OVER)
        core { c ->
            val egonu = c.team(team).players.first { it.name == "Paola Egonu" }.id
            c.scouting(team, match).use { s ->
                // in the front row she may attack or block the ball
                // coming over
                s.choosePlayerFirst(egonu)
                s.chooseEvent(EventTypeEnum.A)
                s.chooseEval(EvalEnum.PERFECT)
            }
        }
        rule.setContent { Scout4AllTheme { MatchStatsScreen(team, match, null, onBack = {}) } }
        rule.waitForText(string(R.string.stats_distribution))
        rule.onNode(hasText(string(R.string.stats_distribution)) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForText(count)
    }
}
