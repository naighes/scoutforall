package io.github.naighes.scout4all

import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.performClick
import io.github.naighes.scout4all.core.MatchForm
import io.github.naighes.scout4all.core.PlayerForm
import io.github.naighes.scout4all.core.RoleEnum
import io.github.naighes.scout4all.core.TeamSideEnum
import android.graphics.Color
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.onRoot
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.abbreviatedName
import org.robolectric.ParameterizedRobolectricTestRunner

/** The side of B receiving, with long names, on every screen. */
@RunWith(ParameterizedRobolectricTestRunner::class)
class LongNamesBothSidesTest(private val screen: Size) {
    @get:Rule(order = 0) val size = SizeRule(screen)
    @get:Rule(order = 1) val rule = createComposeRule()

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun sizes() = sizes.filterNot { it.scrolls }.map { arrayOf(it) }

        private val long = listOf(
            Triple("Alessandro Magrini", RoleEnum.SETTER, 12),
            Triple("Christian Di Balsamo", RoleEnum.OUTSIDE_HITTER, 28),
            Triple("Daniele Bettoni", RoleEnum.MIDDLE_BLOCKER, 17),
            Triple("Alessandro Mogini", RoleEnum.OPPOSITE_HITTER, 27),
            Triple("Alessandro Brilli", RoleEnum.OUTSIDE_HITTER, 13),
            Triple("Alessandro Augero", RoleEnum.MIDDLE_BLOCKER, 16),
            Triple("Riccardo Bartolozzi", RoleEnum.LIBERO, 1),
        )
    }

    private fun scout(rotation: Int = 0): String {
        val team = saveNationalTeam()
        val match = core { c ->
            val b = saveTeam("Arezzo")
            long.forEach { (name, role, number) -> c.savePlayer(b, null, PlayerForm(name, role, number.toString())) }
            val match = c.saveMatch(team, null, MatchForm("Arezzo", "2026-10-25T18:00:00+02:00", true, "6", opponentTeam = b, both = true))
            c.saveRosterOfB(team, match, MatchForm("", null, false, ""))
            val lineup = { start: io.github.naighes.scout4all.core.SetStart, id: String, names: List<String>, setter: String, libero: String ->
                val players = c.team(id).players.associateBy { it.name }
                names.take(6).forEachIndexed { i, name -> start.place((i + 1).toUByte(), players.getValue(name).id) }
                start.chooseSetter(players.getValue(setter).id)
                start.chooseLibero(players.getValue(libero).id)
            }
            val a = c.setStart(team, match)
            a.chooseServingTeam(TeamSideEnum.US)
            lineup(a, team, nationalTeam.map { it.first }, "Alessia Orro", "Monica De Gennaro")
            val ofB = checkNotNull(c.setStartOfB(team, match))
            lineup(ofB, b, (0..5).map { long[(it + rotation) % 6].first }, "Alessandro Magrini", "Riccardo Bartolozzi")
            c.startSetBoth(a, ofB)
            match
        }
        rule.setContent { Scout4AllTheme { ScoutingScreen(team, match, onBack = {}) } }
        rule.waitForText("Arezzo", substring = true)
        return match
    }

    @Test
    fun theSideOfBReceives() {
        scout()
        val plus = hasText("+") and hasClickAction() and isEnabled()
        rule.waitUntil(WAIT_MS) { rule.onAllNodes(plus).fetchSemanticsNodes().size == 1 }
        rule.onNode(plus).performClick()
        rule.waitFor { runCatching { rule.onNode(hasContentDescription(string(R.string.swap_side)) and hasClickAction()).assertIsEnabled() }.isSuccess }
        rule.shot("long-both-reception-$screen")
        // the name of the receiver reads on the floor, not on the jersey of
        // the setter hiding behind: not a light patch
        val image = rule.onRoot().captureToImage().asAndroidBitmap()
        val labels = rule.onAllNodes(SemanticsMatcher("name of 28") { n ->
            n.config.getOrNull(SemanticsProperties.Text)?.joinToString()?.let { it != "28" && it.startsWith("C") } == true
        }, useUnmergedTree = true).fetchSemanticsNodes()
        assertTrue("no name of 28", labels.isNotEmpty())
        val jersey = rule.onAllNodes(SemanticsMatcher("jersey of 12") { n ->
            n.config.getOrNull(SemanticsProperties.Text)?.joinToString() == "12"
        }, useUnmergedTree = true).fetchSemanticsNodes().map { it.bounds() }
        labels.forEach { n ->
            // where the jersey of the setter lies under the name
            val shared = jersey.mapNotNull { j -> n.bounds().intersect(j).takeIf { it.width > 0 && it.height > 0 } }
            shared.forEach { r ->
                val pixels = (r.left.toInt() until r.right.toInt()).flatMap { x -> (r.top.toInt() until r.bottom.toInt()).map { y -> image.getPixel(x, y) } }
                val light = pixels.count { Color.luminance(it) > 0.5f }
                assertTrue("light on the name: $light of ${pixels.size}", light < pixels.size / 2)
            }
        }
    }

    /** Three receivers in a column, 62dp apart: the name of one is not
     * covered by the jersey of the next, it goes in one line. */
    @Test
    fun noNameIsCoveredByAJersey() {
        scout(rotation = 2)
        val plus = hasText("+") and hasClickAction() and isEnabled()
        rule.waitUntil(WAIT_MS) { rule.onAllNodes(plus).fetchSemanticsNodes().size == 1 }
        rule.onNode(plus).performClick()
        rule.waitFor { runCatching { rule.onNode(hasContentDescription(string(R.string.swap_side)) and hasClickAction()).assertIsEnabled() }.isSuccess }
        val texts = { match: (String) -> Boolean ->
            rule.onAllNodes(SemanticsMatcher("text") { n -> n.config.getOrNull(SemanticsProperties.Text)?.joinToString()?.let(match) == true }, useUnmergedTree = true)
                .fetchSemanticsNodes()
        }
        val labels = long.map { it.first }.flatMap { name -> texts { it == name || it == abbreviatedName(name) } }
            // not drawn where no line is free
            .filter { it.config.getOrNull(SemanticsProperties.TestTag) != HIDDEN_NAME }
        assertTrue("no names on the court", labels.size >= 3)
        // the middle of a jersey and a radius inside it
        val radius = with(rule.density) { 16.dp.toPx() }
        val jerseys = long.map { it.third.toString() }.flatMap { number -> texts { it == number } }.map { it.bounds() }
        labels.forEach { label ->
            val covered = jerseys.filter { j ->
                val c = j.center
                c.x in label.bounds().left - radius..label.bounds().right + radius && c.y in label.bounds().top - radius..label.bounds().bottom + radius
            }
            assertTrue("a jersey on the name at ${label.bounds()}: $covered", covered.isEmpty())
        }
    }
}
