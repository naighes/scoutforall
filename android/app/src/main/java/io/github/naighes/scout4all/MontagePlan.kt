package io.github.naighes.scout4all

import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.MontagePiece
import io.github.naighes.scout4all.core.TeamClipMatch
import io.github.naighes.scout4all.core.VideoReview
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** A match in the video of the actions picked: its review (files, how it
 * stands), its pieces, the card before them (none with a single match). */
data class MontageMatch(
    val review: VideoReview,
    val pieces: List<MontagePiece>,
    val team: String,
    val opponent: String,
    /** the opponent, then the day and which match of how many */
    val title: List<String>?,
)

/** Why a match of the team is left out of the video of its actions. */
enum class LeftOut { NO_VIDEO, UNREADABLE, NOT_LINED_UP }

/** The video of the actions picked before making it: the matches with
 * their pieces, the actions counted across them, the matches left out and
 * why (as `Summary` on the desktop). */
data class MontagePlan(
    val matches: List<MontageMatch>,
    val actions: Int,
    val leftOut: List<Pair<String, LeftOut>>,
)

/** The texts of the cards of the matches, in the language of the app. */
data class TitleTexts(
    /** "Match %1$d of %2$d" */
    val matchOf: String,
    /** "Actions: %1$d" */
    val count: String,
)

/** The video of the actions of [request], match after match: the review of
 * the match shown as it is ([shown], its files already read), the others
 * read here; a match without files, with a file gone or not lined up left
 * out. The actions numbered across all the matches. */
suspend fun montagePlan(
    app: Scout4AllApplication,
    request: VideoRequest.Clips,
    shown: Pair<String, VideoReview>?,
    beforeS: UInt,
    afterS: UInt,
    texts: TitleTexts,
): MontagePlan {
    val leftOut = request.leftOut.map { it.opponent to LeftOut.NO_VIDEO }.toMutableList()
    val kept = mutableListOf<Triple<TeamClipMatch, VideoReview, List<MontagePiece>>>()
    request.matches.forEach { m ->
        when (val read = readMatch(app, request, m, shown, beforeS, afterS)) {
            is Read.Kept -> kept += Triple(m, read.review, read.pieces)
            is Read.Out -> leftOut += m.opponent to read.why
        }
    }
    val several = request.matches.size + request.leftOut.size > 1
    val count = kept.sumOf { it.third.size }
    var n = 0
    val matches = kept.mapIndexed { i, (m, review, pieces) ->
        val (team, opponent) = app.onCore { review.team() to review.opponent() }
        val actions = pieces.sumOf { it.actions.toInt() }
        MontageMatch(
            review = review,
            pieces = pieces.map { it.copy(number = (++n).toUInt(), count = count.toUInt()) },
            team = team,
            opponent = opponent,
            title = when (several) {
                true -> listOf(
                    opponent,
                    m.day,
                    texts.matchOf.format(i + 1, kept.size) + " · " + texts.count.format(actions),
                )
                false -> null
            },
        )
    }
    return MontagePlan(matches, matches.sumOf { m -> m.pieces.sumOf { it.actions.toInt() } }, leftOut)
}

private sealed interface Read {
    data class Kept(val review: VideoReview, val pieces: List<MontagePiece>) : Read
    data class Out(val why: LeftOut) : Read
}

/** A match of the actions picked: its review, its files read, its pieces;
 * or why it's left out. */
private suspend fun readMatch(
    app: Scout4AllApplication,
    request: VideoRequest.Clips,
    m: TeamClipMatch,
    shown: Pair<String, VideoReview>?,
    beforeS: UInt,
    afterS: UInt,
): Read {
    val review = when (shown?.first) {
        m.matchId -> shown.second
        else -> try {
            app.core { it.videoReview(request.teamId, m.matchId, request.pick) }
        } catch (_: MobileException) {
            return Read.Out(LeftOut.UNREADABLE)
        }
    }
    val rows = app.onCore { review.files() }
    val durations = when {
        rows.isEmpty() -> return Read.Out(LeftOut.NO_VIDEO)
        app.onCore { review.newer() } -> return Read.Out(LeftOut.UNREADABLE)
        // the match shown: its files already read
        review === shown?.second -> rows.map { it.durationMs?.toLong() }
        else -> withContext(Dispatchers.IO) { rows.map { durationOf(app, it.path) } }
    }
    if (durations.any { it == null }) return Read.Out(LeftOut.UNREADABLE)
    val pieces = try {
        app.onCore {
            if (review !== shown?.second) review.setDurations(durations.filterNotNull().map { it.toULong() })
            review.pieces(beforeS, afterS)
        }
    } catch (_: MobileException) {
        return Read.Out(LeftOut.UNREADABLE)
    }
    return when (pieces.isEmpty()) {
        true -> Read.Out(LeftOut.NOT_LINED_UP)
        false -> Read.Kept(review, pieces)
    }
}
