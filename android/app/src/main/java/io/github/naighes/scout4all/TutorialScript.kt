package io.github.naighes.scout4all

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.github.naighes.scout4all.core.MobileException
import kotlinx.coroutines.launch

/** The tutorial of the activity: by itself once, on a new installation,
 * once the terms are accepted; again from the settings. It scouts a match
 * of the example team, which stays. */
class TutorialViewModel(private val app: Scout4AllApplication) : ViewModel() {
    val tutorial = Tutorial()

    /** Once, if still to show. */
    fun startIfPending() {
        if (tutorial.active) return
        viewModelScope.launch {
            val team = try {
                app.core { core -> if (core.tutorialPending()) core.tutorialTeam() else null }
            } catch (_: MobileException) {
                null
            }
            team?.let(::start)
        }
    }

    private fun start(team: String) = tutorial.start(tutorialSteps(team)) {
        viewModelScope.launch {
            try {
                app.core { it.tutorialSeen() }
            } catch (_: MobileException) {
            }
        }
    }
}

/** From the teams to the scouting of a match of the example `team`. */
fun tutorialSteps(team: String): List<TutorialStep> = listOf(
    TutorialStep.Read(null, R.string.tutorial_welcome),
    TutorialStep.Touch("team:$team", R.string.tutorial_team),
    TutorialStep.Touch("matches-tab", R.string.tutorial_matches),
    TutorialStep.Touch("new-match", R.string.tutorial_new_match),
    TutorialStep.Read("match-fields", R.string.tutorial_match_fields),
    TutorialStep.Touch("save-match", R.string.tutorial_save_match),
    TutorialStep.Read("lineup", R.string.tutorial_lineup),
    TutorialStep.Touch("start-set", R.string.tutorial_start_set),
)
