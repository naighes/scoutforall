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

/** From the teams to the scouting of a match of the example `team`, its
 * statistics last. */
fun tutorialSteps(team: String): List<TutorialStep> = toTheSet(team) + rallies + lineupChanges + listOf(
    TutorialStep.SwipeUp("stats", until = STATS_OPENED, caption = R.string.tutorial_stats),
    TutorialStep.Done(R.string.tutorial_done),
)

/** The example team, a new match, its first set. */
private fun toTheSet(team: String) = listOf(
    TutorialStep.Read(null, R.string.tutorial_welcome),
    TutorialStep.Touch("team:$team", R.string.tutorial_team),
    TutorialStep.Touch("matches-tab", R.string.tutorial_matches),
    TutorialStep.Touch("new-match", R.string.tutorial_new_match),
    TutorialStep.Read("match-fields", R.string.tutorial_match_fields),
    TutorialStep.Touch("save-match", R.string.tutorial_save_match),
    TutorialStep.Read("lineup", R.string.tutorial_lineup),
    TutorialStep.Touch("start-set", R.string.tutorial_start_set),
)

private fun touch(target: String, caption: Int) = TutorialStep.Touch(target, caption)

// the players by number: 16 Nervini, 11 Danesi, 19 Fahr, 18 Egonu, 8 Orro,
// 3 Cambi, 24 Antropova (`core::example`)

/** Two rallies: a side-out, then a break (2-0). */
private val rallies = listOf(
    TutorialStep.Read("score", R.string.tutorial_score),
    // the first: Nervini receives and scores
    touch("player:16", R.string.tutorial_reception),
    touch("eval:PERFECT", R.string.tutorial_reception_eval),
    touch("player:16", R.string.tutorial_attack),
    touch("event:A", R.string.tutorial_attack_event),
    touch("eval:PERFECT", R.string.tutorial_attack_point),
    TutorialStep.Read("score", R.string.tutorial_side_out),
    // the second: Fahr serves, a long rally, their error
    touch("eval:NEGATIVE", R.string.tutorial_serve),
    touch("player:11", R.string.tutorial_block),
    touch("event:B", R.string.tutorial_block_event),
    touch("eval:NEGATIVE", R.string.tutorial_block_eval),
    touch("player:19", R.string.tutorial_dig),
    touch("event:D", R.string.tutorial_dig_event),
    touch("eval:POSITIVE", R.string.tutorial_dig_eval),
    touch("player:18", R.string.tutorial_opposite),
    touch("event:A", R.string.tutorial_attack_event),
    touch("eval:NEGATIVE", R.string.tutorial_attack_dug),
    touch("event:OF", R.string.tutorial_their_freeball),
    touch("player:18", R.string.tutorial_opposite_again),
    touch("event:A", R.string.tutorial_attack_event),
    touch("eval:NEGATIVE", R.string.tutorial_attack_dug),
    touch("event:OE", R.string.tutorial_their_error),
)

/** A double substitution, the setter that came in, the other libero. */
private val lineupChanges = listOf(
    touch("lineup", R.string.tutorial_lineup_menu),
    touch("event:R", R.string.tutorial_substitution),
    touch("player:18", R.string.tutorial_out_egonu),
    touch("replacement:3", R.string.tutorial_in_cambi),
    touch("lineup", R.string.tutorial_lineup_again),
    touch("event:R", R.string.tutorial_substitution),
    touch("player:8", R.string.tutorial_out_orro),
    touch("replacement:24", R.string.tutorial_in_antropova),
    touch("lineup", R.string.tutorial_setter_menu),
    touch("event:CS", R.string.tutorial_setter),
    touch("player:3", R.string.tutorial_setter_cambi),
    touch("lineup", R.string.tutorial_libero_menu),
    touch("event:CL", R.string.tutorial_libero),
)
