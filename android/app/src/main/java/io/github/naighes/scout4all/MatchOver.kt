package io.github.naighes.scout4all

import android.provider.Settings
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.AlertDialogDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.rotate
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import io.github.naighes.scout4all.core.MatchResult
import io.github.naighes.scout4all.core.ScoutingState
import kotlin.math.sin
import kotlin.random.Random

/** The last set ended the match: the set, then the match with its sets and
 * the points of each, a word for the team. Won, the trophy and a short
 * fall of confetti, once; lost, nothing that moves. Back to the match, or
 * undo the last point if it was a mistake. */
@Composable
fun MatchOverDialog(state: ScoutingState, result: MatchResult, onUndo: () -> Unit, onDone: () -> Unit) {
    val won = result.won > result.lost
    Dialog(
        // the back of the system as OK: the match is saved
        onDismissRequest = onDone,
        properties = DialogProperties(dismissOnClickOutside = false, usePlatformDefaultWidth = false),
    ) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            Surface(
                Modifier.padding(24.dp).widthIn(max = 400.dp),
                shape = AlertDialogDefaults.shape,
                color = AlertDialogDefaults.containerColor,
                // not the amber Material gives the grey of the tiles, which is
                // also `primaryContainer` in this theme
                contentColor = MaterialTheme.colorScheme.onSurface,
                tonalElevation = AlertDialogDefaults.TonalElevation,
            ) {
                Column(Modifier.padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    if (won) {
                        Icon(
                            painterResource(R.drawable.ic_trophy),
                            contentDescription = null,
                            Modifier.size(40.dp),
                            tint = MaterialTheme.colorScheme.primary,
                        )
                    }
                    Text(
                        stringResource(if (won) R.string.match_won else R.string.match_lost),
                        Modifier.padding(top = 8.dp).title(),
                        style = MaterialTheme.typography.headlineSmall,
                    )
                    Text(
                        "${result.won}–${result.lost}",
                        Modifier.readAs(stringResource(R.string.sets_score, result.won.toInt(), result.lost.toInt())),
                        style = MaterialTheme.typography.displayMedium,
                        fontWeight = FontWeight.SemiBold,
                        color = if (won) Won else Lost,
                    )
                    // the points of each set, the team's first
                    Text(
                        result.sets.joinToString("  ") { "${it.us}–${it.them}" },
                        Modifier.readAs(
                            result.sets.map { stringResource(R.string.score_to, it.us.toInt(), it.them.toInt()) }
                                .joinToString(", "),
                        ),
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        textAlign = TextAlign.Center,
                    )
                    Text(
                        stringResource(if (won) R.string.match_won_cheer else R.string.match_lost_cheer),
                        Modifier.padding(top = 16.dp),
                        style = MaterialTheme.typography.bodyLarge,
                        textAlign = TextAlign.Center,
                    )
                    Row(
                        Modifier.fillMaxWidth().padding(top = 24.dp),
                        horizontalArrangement = Arrangement.End,
                    ) {
                        TextButton(onClick = onUndo) { Text(stringResource(R.string.undo)) }
                        TextButton(onClick = onDone) { Text(stringResource(R.string.ok)) }
                    }
                }
            }
            if (won) Confetti(state.setNumber.toInt())
        }
    }
}

/** A few confetti falling over the dialog, once for the match (`key`): not
 * again when the screen is made again. None when the system removes the
 * animations. */
@Composable
private fun Confetti(key: Int) {
    val context = LocalContext.current
    val animations = remember {
        Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) > 0f
    }
    var fallen by rememberSaveable(key) { mutableStateOf(!animations) }
    if (fallen) return
    val pieces = remember { List(CONFETTI) { Piece(Random(it)) } }
    val time = remember { Animatable(0f) }
    LaunchedEffect(Unit) {
        time.animateTo(1f, tween(CONFETTI_MILLIS, easing = LinearEasing))
        fallen = true
    }
    val colors = listOf(MaterialTheme.colorScheme.primary, Won, MaterialTheme.colorScheme.onSurface)
    Canvas(Modifier.fillMaxSize()) {
        val t = time.value
        pieces.forEach { p ->
            val y = (p.start + t * p.speed) * size.height
            val x = (p.x + p.sway * sin((t * 6 + p.phase) * Math.PI).toFloat()) * size.width
            // fading in the last third
            val alpha = ((1f - t) * 3f).coerceIn(0f, 1f)
            rotate(p.turn + t * p.spin, Offset(x, y)) {
                drawRect(
                    colors[p.color % colors.size].copy(alpha = alpha),
                    topLeft = Offset(x - 4.dp.toPx(), y - 2.dp.toPx()),
                    size = Size(8.dp.toPx(), 4.dp.toPx()),
                )
            }
        }
    }
}

private const val CONFETTI = 48
private const val CONFETTI_MILLIS = 2000

/** A piece of confetti: where it starts (fractions of the screen, above
 * it), how fast it falls and sways, how it turns, its color. */
private class Piece(r: Random) {
    val x = r.nextFloat()
    val start = -0.1f - r.nextFloat() * 0.3f
    val speed = 0.7f + r.nextFloat() * 0.6f
    val sway = 0.01f + r.nextFloat() * 0.03f
    val phase = r.nextFloat() * 2
    val turn = r.nextFloat() * 360
    val spin = 180 + r.nextFloat() * 540
    val color = r.nextInt(3)
}
