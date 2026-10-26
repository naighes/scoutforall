package io.github.naighes.scout4all

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner

/** The tutorial in each language: the rotations its captions name are the
 * ones the screen shows (S6 in English, P6 in Italian). */
@RunWith(ParameterizedRobolectricTestRunner::class)
class TutorialLanguagesTest(private val language: String) {
    @get:Rule(order = 0) val size = SizeRule(Size(language, "$language-w392dp-h800dp-port"))
    @get:Rule(order = 1) val rule = createAndroidComposeRule<MainActivity>()

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun languages() = listOf("en", "it", "pt", "pl", "ja").map { arrayOf(it) }
    }

    @Test
    fun theCaptionsNameTheRotationsShown() {
        core { it.addExampleTeamIfNew() }
        rule.waitForText(string(R.string.accept_terms))
        rule.onNode(hasText(string(R.string.accept_terms)) and hasClickAction()).performClick()
        val rotation = Regex("""\b\p{Lu}[1-6]\b""")
        val named = mutableSetOf<String>()
        val wrong = mutableListOf<String>()
        rule.playTutorial(rule.tutorial) { step, _ ->
            val caption = string(step.caption)
            rotation.findAll(caption).map { it.value }.forEach { r ->
                named += r
                val shown = step.target != "score" || rule.onAllNodes(hasText("· $r", substring = true)).fetchSemanticsNodes().isNotEmpty()
                if (!shown) wrong += "$r not shown: $caption"
            }
        }
        // the lineup names one before the scouting shows it: the same letter
        val letters = named.map { it.first() }.distinct()
        assertEquals(emptyList<String>(), wrong + letters.drop(1).map { "another letter: $it in $named" })
    }
}
