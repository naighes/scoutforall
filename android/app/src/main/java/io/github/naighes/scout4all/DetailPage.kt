package io.github.naighes.scout4all

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowDropDown
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.DetailFilter
import io.github.naighes.scout4all.core.EvalEnum
import io.github.naighes.scout4all.core.SituationChoices
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.material3.HorizontalDivider
import io.github.naighes.scout4all.core.DetailStats
import io.github.naighes.scout4all.core.EvalStats
import io.github.naighes.scout4all.core.EventTypeEnum
import io.github.naighes.scout4all.core.FreeballStats
import io.github.naighes.scout4all.core.FundamentalDetail
import io.github.naighes.scout4all.core.PlayerStats
import io.github.naighes.scout4all.core.Tone
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.heightIn
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.graphics.luminance
import io.github.naighes.scout4all.core.TargetStats
import io.github.naighes.scout4all.core.ServeStartEnum
import io.github.naighes.scout4all.core.CourtZoneEnum
import androidx.compose.foundation.border
import kotlin.math.roundToInt

/** What of the detail is picked to watch in the video: some evaluations,
 * a zone of the net, a zone of the court (one each). */
data class ClipChoice(
    val evals: List<EvalEnum> = emptyList(),
    val zone: UByte? = null,
    val target: CourtZoneEnum? = null,
) {
    val empty: Boolean get() = evals.isEmpty() && zone == null && target == null

    fun toggled(eval: EvalEnum) = copy(evals = if (eval in evals) evals - eval else evals + eval)
    fun toggled(zone: UByte) = copy(zone = if (this.zone == zone) null else zone)
    fun toggled(target: CourtZoneEnum) = copy(target = if (this.target == target) null else target)
}

/** Watching the actions of the detail in the video, on top of it: how many
 * the choice keeps and the button (the match's or the team's). None while
 * scouting. */
class Watch(val header: @Composable (detail: DetailStats, query: DetailQuery, choice: ClipChoice, clear: () -> Unit) -> Unit)

/** The detail asked for: a fundamental, narrowed by the filter. */
data class DetailQuery(val event: EventTypeEnum = EventTypeEnum.S, val filter: DetailFilter = DetailFilter(null, null, null))

/** How good an evaluation is, on the colors of the grades. */
private fun toneColor(tone: Tone) = gradeColor(
    when (tone) {
        Tone.EXCELLENT -> 1.0
        Tone.GOOD -> 0.75
        Tone.FAIR -> 0.5
        Tone.POOR -> 0.25
        Tone.BAD -> 0.0
    },
)


/** A fundamental in detail: chosen on a chip, narrowed to a player, a
 * rotation, a phase; how its evaluations went, its figures, the zones of the
 * attack, and the players compared (touch one to narrow to it). */
@Composable
fun Detail(
    detail: DetailStats?,
    query: DetailQuery,
    situation: SituationChoices? = null,
    // the analysis has them over every page
    filters: Boolean = true,
    // the actions to watch in the video, picked touching the evaluations
    // and the zones: anew for each fundamental
    watch: Watch? = null,
    onQuery: (DetailQuery) -> Unit,
) {
    if (detail == null) return
    val filter = query.filter
    var choice by remember(query.event) { mutableStateOf(ClipChoice()) }
    Row(
        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        detail.fundamentals.forEach { f ->
            FilterChip(
                selected = f.value == query.event,
                onClick = { onQuery(query.copy(event = f.value)) },
                label = { Text(f.name.replaceFirstChar { it.uppercase() }) },
            )
        }
    }
    if (filters) DetailFilters(detail, situation, filter) { onQuery(query.copy(filter = it)) }
    watch?.header?.invoke(detail, query, choice) { choice = ClipChoice() }
    detail.fundamental?.let { f ->
        FundamentalContent(f, query, onQuery, choice.takeIf { watch != null }) { choice = it }
    }
    detail.freeballs?.let { FreeballContent(it) }
}

/** The filters of the statistics: the player, the rotation, the phase and,
 * with its choices, the situation of the game (the role, the reception of
 * the rally, the touch before, the set, the reception formation, the P1),
 * on as many lines as they take. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun DetailFilters(detail: DetailStats, situation: SituationChoices?, filter: DetailFilter, onFilter: (DetailFilter) -> Unit) =
    FlowRow(
        Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        FilterMenu(
            stringResource(R.string.filter_player),
            detail.players.find { it.id == filter.player }?.let { "${it.number} ${it.name}" },
            detail.players.map { "${it.number} ${it.name}" to it.id },
        ) { onFilter(filter.copy(player = it)) }
        FilterMenu(
            stringResource(R.string.filter_rotation),
            detail.rotations.find { it.value == filter.rotation }?.name,
            detail.rotations.map { it.name to it.value },
        ) { onFilter(filter.copy(rotation = it)) }
        FilterMenu(
            stringResource(R.string.filter_phase),
            detail.phases.find { it.value == filter.phase }?.name,
            detail.phases.map { it.name to it.value },
        ) { onFilter(filter.copy(phase = it)) }
        if (situation == null) return@FlowRow
        FilterMenu(
            stringResource(R.string.role),
            situation.roles.find { it.value == filter.role }?.name,
            situation.roles.map { it.name to it.value },
        ) { onFilter(filter.copy(role = it)) }
        val groups = evalGroups(situation)
        val symbols = situation.symbols.map { it.symbol to it.value }
        ManyFilterMenu(stringResource(R.string.rally_reception), filter.reception, groups, symbols, { e -> symbolsOf(situation, e) }) {
            onFilter(filter.copy(reception = it))
        }
        ManyFilterMenu(stringResource(R.string.touch_before), filter.previous, groups, symbols, { e -> symbolsOf(situation, e) }) {
            onFilter(filter.copy(previous = it))
        }
        ManyFilterMenu(
            stringResource(R.string.set_type),
            filter.setTypes,
            emptyList(),
            situation.setTypes.map { "${it.code}  ${it.name}" to it.value },
            { chosen -> situation.setTypes.filter { it.value in chosen }.joinToString(" ") { it.code } },
        ) { onFilter(filter.copy(setTypes = it)) }
        FilterMenu(
            stringResource(R.string.reception_by),
            situation.schemes.find { it.value == filter.scheme }?.let {
                stringResource(R.string.reception_scheme) + " " + stringResource(R.string.receivers, it.receivers.toInt())
            },
            situation.schemes.map { stringResource(R.string.receivers, it.receivers.toInt()) to it.value },
        ) { onFilter(filter.copy(scheme = it)) }
        val p1 = listOf(stringResource(R.string.canonical) to false, stringResource(R.string.inverted) to true)
        FilterMenu(
            "P1",
            p1.find { it.second == filter.invertedP1 }?.let { "P1 ${it.first}" },
            p1,
        ) { onFilter(filter.copy(invertedP1 = it)) }
        val moments = listOf(stringResource(R.string.before_20) to false, stringResource(R.string.from_20) to true)
        FilterMenu(
            stringResource(R.string.set_moment),
            moments.find { it.second == filter.late }?.first,
            moments,
        ) { onFilter(filter.copy(late = it)) }
        val gaps = listOf(stringResource(R.string.close_score) to true, stringResource(R.string.wide_score) to false)
        FilterMenu(
            stringResource(R.string.score_gap),
            gaps.find { it.second == filter.close }?.let { stringResource(R.string.score_gap) + " " + it.first },
            gaps,
        ) { onFilter(filter.copy(close = it)) }
    }

/** The evaluations chosen together: the good ones, `!`, the bad ones. */
@Composable
private fun evalGroups(situation: SituationChoices): List<Pair<String, List<EvalEnum>>> =
    situation.groups.map { g ->
        val name = when {
            g.evals.contains(EvalEnum.PERFECT) -> stringResource(R.string.positive_group) + " "
            g.evals.contains(EvalEnum.OVER) -> stringResource(R.string.negative_group) + " "
            else -> ""
        }
        name + g.symbols to g.evals
    }

/** The symbols of the evaluations chosen, in their order. */
private fun symbolsOf(situation: SituationChoices, chosen: List<EvalEnum>): String =
    situation.symbols.filter { it.value in chosen }.joinToString("") { it.symbol }

/** A chip naming the filter, or what's chosen; a menu to choose more of
 * them: "all", the groups (just them), each one on or off. */
@Composable
private fun <T> ManyFilterMenu(
    label: String,
    chosen: List<T>,
    groups: List<Pair<String, List<T>>>,
    options: List<Pair<String, T>>,
    shown: (List<T>) -> String,
    onChoose: (List<T>) -> Unit,
) = Box {
    var open by remember { mutableStateOf(false) }
    FilterChip(
        selected = chosen.isNotEmpty(),
        onClick = { open = true },
        label = {
            Text(
                if (chosen.isEmpty()) label else "$label ${shown(chosen)}",
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        },
        trailingIcon = { Icon(Icons.Filled.ArrowDropDown, contentDescription = null) },
    )
    val tick = @Composable { on: Boolean ->
        if (on) Icon(Icons.Filled.Check, contentDescription = null) else Spacer(Modifier.size(24.dp))
    }
    Menu(expanded = open, onDismissRequest = { open = false }) {
        DropdownMenuItem(
            text = { Text(stringResource(R.string.filter_all)) },
            leadingIcon = { tick(chosen.isEmpty()) },
            onClick = {
                open = false
                onChoose(emptyList())
            },
        )
        groups.forEach { (name, values) ->
            DropdownMenuItem(
                text = { Text(name) },
                leadingIcon = { tick(chosen.toSet() == values.toSet()) },
                onClick = {
                    open = false
                    onChoose(values)
                },
            )
        }
        if (groups.isNotEmpty()) HorizontalDivider()
        // one at a time, the menu stays open
        options.forEach { (name, value) ->
            DropdownMenuItem(
                text = { Text(name) },
                leadingIcon = { tick(value in chosen) },
                onClick = { onChoose(if (value in chosen) chosen - value else chosen + value) },
            )
        }
    }
}

/** A chip naming the filter, or what it's set to; a menu to choose, "all"
 * first. */
@Composable
private fun <T> FilterMenu(label: String, chosen: String?, options: List<Pair<String, T>>, onChoose: (T?) -> Unit) =
    Box {
        var open by remember { mutableStateOf(false) }
        FilterChip(
            selected = chosen != null,
            onClick = { open = true },
            label = { Text(chosen ?: label, maxLines = 1, overflow = TextOverflow.Ellipsis) },
            trailingIcon = { Icon(Icons.Filled.ArrowDropDown, contentDescription = null) },
        )
        // the one chosen with a tick
        val tick = @Composable { on: Boolean ->
            if (on) Icon(Icons.Filled.Check, contentDescription = null) else Spacer(Modifier.size(24.dp))
        }
        Menu(expanded = open, onDismissRequest = { open = false }) {
            DropdownMenuItem(
                text = { Text(stringResource(R.string.filter_all)) },
                leadingIcon = { tick(chosen == null) },
                onClick = {
                    open = false
                    onChoose(null)
                },
            )
            options.forEach { (name, value) ->
                DropdownMenuItem(
                    text = { Text(name) },
                    leadingIcon = { tick(chosen == name) },
                    onClick = {
                        open = false
                        onChoose(value)
                    },
                )
            }
        }
    }

@Composable
private fun FundamentalContent(
    f: FundamentalDetail,
    query: DetailQuery,
    onQuery: (DetailQuery) -> Unit,
    // none: nothing to pick
    choice: ClipChoice?,
    onChoice: (ClipChoice) -> Unit,
) {
    if (f.total == 0u) {
        Text(stringResource(R.string.no_events), color = MaterialTheme.colorScheme.onSurfaceVariant)
        return
    }
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        DetailTile(
            stringResource(R.string.efficiency).replaceFirstChar { it.uppercase() },
            pct(f.efficiency),
            f.grade?.let { gradeColor(it) },
            Modifier.weight(1f),
        )
        DetailTile(
            stringResource(R.string.positiveness).replaceFirstChar { it.uppercase() },
            pct(f.positiveness),
            null,
            Modifier.weight(1f),
        )
    }
    // the longest bar across, to tell them apart
    val most = f.evals.maxOfOrNull { it.share }?.coerceAtLeast(1.0) ?: 1.0
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        f.evals.forEach { e ->
            EvalBar(e, most, choice?.let { it.evals.contains(e.eval) }) { choice?.let { onChoice(it.toggled(e.eval)) } }
        }
    }
    TileRow {
        DetailTile(stringResource(R.string.total), f.total.toString(), null, Modifier.weight(1f))
        DetailTile(
            stringResource(R.string.stats_errors),
            f.errors.toString(),
            if (f.unforcedErrors > 0u) Lost else null,
            Modifier.weight(1f),
            stringResource(R.string.unforced_detail, f.unforcedErrors.toInt()),
        )
        f.points?.let {
            DetailTile(stringResource(R.string.points), it.toString(), null, Modifier.weight(1f))
        }
    }
    TileRow {
        DetailTile(stringResource(R.string.error_fault), f.faults.toString(), null, Modifier.weight(1f))
        f.phasesPerPoint?.let {
            DetailTile(
                stringResource(R.string.rallies_per_point),
                "%.2f".format(it.perPoint),
                null,
                Modifier.weight(1f),
                stringResource(R.string.rallies_detail, it.won.toInt(), it.rallies.toInt()),
            )
        } ?: Spacer(Modifier.weight(1f))
    }
    if (f.zones.isNotEmpty()) {
        DistributionCourt { zone ->
            val z = f.zones.find { it.zone.toInt() == zone }
            Column(
                Modifier.picked(choice?.let { it.zone?.toInt() == zone }) { choice?.let { onChoice(it.toggled(zone.toUByte())) } },
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text(zone.toString(), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text(
                    pct(z?.share),
                    style = MaterialTheme.typography.headlineSmall,
                    fontWeight = FontWeight.Bold,
                    color = if (z == null) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface,
                )
                z?.let {
                    Text(
                        stringResource(R.string.kill_rate, pct(it.conversion)),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
    if (f.serveStarts.isNotEmpty()) ServeStarts(f, query.filter.serveStart) { start ->
        val toggled = if (query.filter.serveStart == start) null else start
        onQuery(query.copy(filter = query.filter.copy(serveStart = toggled)))
    }
    if (f.serveStarts.isNotEmpty()) ServePaths(
        f,
        query.filter.serveStart,
        query.filter.serveStart?.let { s -> choice?.target?.let { s to it } },
        onStart = { start ->
            val toggled = if (query.filter.serveStart == start) null else start
            onQuery(query.copy(filter = query.filter.copy(serveStart = toggled)))
        },
        onPath = { start, zone, again ->
            // the arrow again: all of them, from every zone
            choice?.let { onChoice(it.copy(target = zone.takeIf { !again })) }
            onQuery(query.copy(filter = query.filter.copy(serveStart = start.takeIf { !again })))
        },
    )
    if (f.targets.isNotEmpty()) TargetZones(f, query.event, query.filter.serveStart, choice, onChoice)
    if (f.blockers.isNotEmpty()) Blockers(f, query.event)
    if (f.players.isNotEmpty()) {
        Text(stringResource(R.string.players_title), Modifier.title(), style = MaterialTheme.typography.titleMedium)
        Column {
            f.players.forEach { p ->
                PlayerRow(p, selected = p.player.id == query.filter.player) {
                    val player = if (p.player.id == query.filter.player) null else p.player.id
                    onQuery(query.copy(filter = query.filter.copy(player = player)))
                }
            }
        }
    }
}

/** Tiles side by side, as tall as the tallest. */
@Composable
private fun TileRow(content: @Composable RowScope.() -> Unit) = Row(
    Modifier.height(IntrinsicSize.Min),
    horizontalArrangement = Arrangement.spacedBy(12.dp),
    content = content,
)

/** Its texts wrap: the large system font makes them longer than the
 * tile. */
@Composable
private fun DetailTile(label: String, value: String, color: Color?, modifier: Modifier, detail: String? = null) = Surface(
    modifier.fillMaxHeight().semantics(mergeDescendants = true) {},
    shape = MaterialTheme.shapes.medium,
    color = MaterialTheme.colorScheme.surfaceVariant,
) {
    Column(Modifier.padding(12.dp)) {
        Text(
            label,
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Text(
            value,
            style = MaterialTheme.typography.headlineSmall,
            fontWeight = FontWeight.Bold,
            color = color ?: MaterialTheme.colorScheme.onSurface,
        )
        detail?.let {
            Text(
                it,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/** An evaluation: its symbol and name, a bar as long as its share. */
@Composable
private fun EvalBar(e: EvalStats, most: Double, picked: Boolean?, onPick: () -> Unit) = Row(
    // a finger's room when it can be touched
    Modifier.then(if (picked == null) Modifier else Modifier.heightIn(min = 48.dp))
        .picked(picked, onPick).semantics(mergeDescendants = true) {},
    verticalAlignment = Alignment.CenterVertically,
) {
    // its name next to it
    Text(
        e.symbol,
        Modifier.width(20.dp).notRead(),
        style = MaterialTheme.typography.titleMedium,
        fontWeight = FontWeight.Bold,
        color = toneColor(e.tone),
    )
    Text(
        e.name,
        Modifier.width(112.dp),
        style = MaterialTheme.typography.bodyMedium,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
    Box(Modifier.weight(1f).height(12.dp)) {
        Box(
            Modifier.fillMaxHeight().fillMaxWidth((e.share / most).toFloat().coerceIn(0f, 1f))
                .clip(RoundedCornerShape(50)).background(toneColor(e.tone)),
        )
    }
    Text(
        "${pct(e.share)} (${e.count})",
        Modifier.width(76.dp),
        textAlign = TextAlign.End,
        style = MaterialTheme.typography.bodyMedium,
        fontWeight = FontWeight.Bold,
    )
}

/** A player in the fundamental: the number, the name, the touches, points
 * and errors, the efficiency in its grade. */
@Composable
private fun PlayerRow(p: PlayerStats, selected: Boolean, onClick: () -> Unit) = Row(
    Modifier.fillMaxWidth().clip(MaterialTheme.shapes.small)
        .background(if (selected) MaterialTheme.colorScheme.surfaceVariant else Color.Transparent)
        .clickable(onClick = onClick).padding(vertical = 8.dp, horizontal = 4.dp),
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(12.dp),
) {
    Box(
        Modifier.size(32.dp).clip(CircleShape).background(MaterialTheme.colorScheme.onSurface),
        contentAlignment = Alignment.Center,
    ) {
        val size = 14.dp.jerseySp()
        Text(
            p.player.number.toString(),
            style = MaterialTheme.typography.labelLarge.copy(fontSize = size, lineHeight = size),
            fontWeight = FontWeight.Bold,
            color = MaterialTheme.colorScheme.surface,
        )
    }
    Column(Modifier.weight(1f)) {
        Text(p.player.name, style = MaterialTheme.typography.bodyMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(
            listOfNotNull(
                p.total.toString(),
                p.points?.let { "#$it" },
                "err ${p.errors}",
            ).joinToString(" · "),
            Modifier.readAs(
                listOfNotNull(
                    "${stringResource(R.string.total)} ${p.total}",
                    p.points?.let { "${stringResource(R.string.points)} $it" },
                    "${stringResource(R.string.stats_errors)} ${p.errors}",
                ).joinToString(", "),
            ),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
    Text(
        pct(p.efficiency),
        style = MaterialTheme.typography.titleMedium,
        fontWeight = FontWeight.Bold,
        color = p.grade?.let { gradeColor(it) } ?: MaterialTheme.colorScheme.onSurface,
    )
}

/** The freeballs we gave, the ones they gave us and how many we
 * exploited: theirs have no player. */
@Composable
private fun FreeballContent(f: FreeballStats) = TileRow {
    DetailTile(stringResource(R.string.freeballs_given), f.given.toString(), null, Modifier.weight(1f))
    f.received?.let {
        DetailTile(stringResource(R.string.freeballs_received), it.toString(), null, Modifier.weight(1f))
    }
    f.exploited?.let {
        DetailTile(
            stringResource(R.string.freeballs_exploited),
            pct(it.percentage),
            null,
            Modifier.weight(1f),
            stringResource(R.string.rallies_detail, it.exploited.toInt(), it.total.toInt()),
        )
    }
}

/** The serves or attacks with their zone told, on their court, or the
 * receptions or digs, on ours, or the blocks at our net: where they went, the more the brighter
 * amber; or their efficiency there, red to green, from a few on. Two cells
 * of two zones count half in each. On our court also the conflict zones,
 * between the zones, and all of them under the court. */
@Composable
private fun TargetZones(
    f: FundamentalDetail,
    event: EventTypeEnum,
    from: ServeStartEnum?,
    choice: ClipChoice?,
    onChoice: (ClipChoice) -> Unit,
) = Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
    var efficiency by rememberSaveable { mutableStateOf(false) }
    val (title, told) = when (event) {
        EventTypeEnum.P -> R.string.receptions_by_zone to R.string.reception_zones_told
        EventTypeEnum.A -> R.string.attacks_by_zone to R.string.attack_zones_told
        EventTypeEnum.D -> R.string.digs_by_zone to R.string.dig_zones_told
        EventTypeEnum.B -> R.string.blocks_by_zone to R.string.block_zones_told
        else -> R.string.serves_by_zone to R.string.zones_told
    }
    // only the serves from the zone picked above
    val start = f.serveStarts.firstOrNull { it.value == from }?.takeIf { event == EventTypeEnum.S }
    val named = listOfNotNull(stringResource(title), start?.let { stringResource(R.string.serve_from, it.label) })
    Text(named.joinToString(" · "), Modifier.title(), style = MaterialTheme.typography.titleMedium)
    Text(
        stringResource(told, f.targetsTold.toInt(), f.total.toInt()),
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
        listOf(R.string.zone_density to false, R.string.efficiency to true).forEachIndexed { i, (name, shows) ->
            SegmentedButton(
                selected = efficiency == shows,
                onClick = { efficiency = shows },
                shape = SegmentedButtonDefaults.itemShape(i, 2),
            ) { Text(stringResource(name).replaceFirstChar { it.uppercase() }, maxLines = 1) }
        }
    }
    val most = f.targets.flatten().maxOfOrNull { it.count }?.coerceAtLeast(1.0) ?: 1.0
    ZoneGrid(f.targets) { t ->
        val (background, value, detail) = when {
            !efficiency -> Triple(
                densityColor(t.count / most).takeIf { t.count > 0.0 },
                if (t.count > 0.0) "${t.share.roundToInt()}%" else "",
                if (t.count > 0.0) "(${count(t.count)})" else "",
            )
            t.efficiency != null -> Triple(
                gradeColor((t.efficiency!! + 100.0) / 200.0),
                "%+d%%".format(t.efficiency!!.roundToInt()),
                "n=${count(t.count)}",
            )
            // too few to tell
            else -> Triple(null, if (t.count > 0.0) "(${count(t.count)})" else "", "")
        }
        Box(Modifier.fillMaxSize().picked(choice?.let { it.target == t.value }) { choice?.let { onChoice(it.toggled(t.value)) } }) {
            ZoneCell(t, background, value, detail)
        }
    }
    f.onConflicts?.let { on ->
        val all = stringResource(R.string.on_conflicts, on.count.toInt(), on.share.roundToInt())
        Text(
            all + (on.efficiency?.takeIf { efficiency }?.let { " · %+d%%".format(it.roundToInt()) } ?: ""),
            style = MaterialTheme.typography.bodyMedium,
        )
    }
}

/** Our serves by the zone they started from, as our server sees our end
 * line (5 on the left, 1 on the right): how many, the more the brighter
 * amber, their share and, from a few on, their efficiency. Touched, where
 * they went only from there (their actions to watch too); again, all. */
@Composable
private fun ServeStarts(f: FundamentalDetail, picked: ServeStartEnum?, onPick: (ServeStartEnum) -> Unit) =
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(stringResource(R.string.serves_by_start), Modifier.title(), style = MaterialTheme.typography.titleMedium)
        Text(
            stringResource(R.string.serve_starts_told, f.serveStartsTold.toInt(), f.total.toInt()),
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        val most = f.serveStarts.maxOfOrNull { it.count.toDouble() }?.coerceAtLeast(1.0) ?: 1.0
        val description = stringResource(R.string.serve_start)
        Row(Modifier.fillMaxWidth().height(IntrinsicSize.Min).heightIn(min = 72.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            f.serveStarts.forEach { z ->
                val some = z.count > 0u
                val value = if (some) "${z.share.roundToInt()}%" else ""
                val detail = z.efficiency?.let { "%+d%%".format(it.roundToInt()) } ?: if (some) "(${z.count})" else ""
                val background = densityColor(z.count.toDouble() / most).takeIf { some }
                val pickable = some || picked == z.value
                Box(
                    Modifier.weight(1f).fillMaxHeight()
                        .picked((picked == z.value).takeIf { pickable }) { onPick(z.value) },
                ) {
                    ZoneCell("$description ${z.label}", z.label, true, background, value, detail)
                }
            }
        }
    }

/** Our attacks by how many of theirs blocked, or our blocks by how many of
 * ours, of the ones told: a row each, how many, their share and, from a
 * few on, their efficiency. */
@Composable
private fun Blockers(f: FundamentalDetail, event: EventTypeEnum) = Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
    val title = if (event == EventTypeEnum.B) R.string.blocks_by_blockers else R.string.attacks_by_block
    Text(stringResource(title), Modifier.title(), style = MaterialTheme.typography.titleMedium)
    Text(
        stringResource(R.string.blockers_told, f.blockersTold.toInt(), f.total.toInt()),
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    f.blockers.forEach { b ->
        val shown = b.count > 0u
        val color = if (shown) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant
        Row(Modifier.fillMaxWidth().heightIn(min = 40.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(b.name, Modifier.weight(1f), style = MaterialTheme.typography.bodyLarge, color = color)
            Text(
                if (shown) "${b.share.roundToInt()}% (${b.count})" else "–",
                style = MaterialTheme.typography.bodyLarge,
                color = color,
            )
            b.efficiency?.let {
                Text(
                    "%+d%%".format(it.roundToInt()),
                    Modifier.padding(start = 12.dp),
                    style = MaterialTheme.typography.bodyLarge,
                    fontWeight = FontWeight.Bold,
                    color = gradeColor((it + 100.0) / 200.0),
                )
            }
        }
    }
}

/** What can be picked to watch in the video: touched, it's picked or not;
 * picked, an amber border. Nothing when there's nothing to pick
 * (`picked` none). */
@Composable
private fun Modifier.picked(picked: Boolean?, onPick: () -> Unit): Modifier = when (picked) {
    null -> this
    else -> this.clip(MaterialTheme.shapes.small)
        .border(2.dp, if (picked) MaterialTheme.colorScheme.primary else Color.Transparent, MaterialTheme.shapes.small)
        .clickable(onClick = onPick)
}

/** How many in a zone: whole, or with the half of two cells of two zones
 * ("2.5"). */
internal fun count(count: Double): String =
    if (count % 1.0 == 0.0) count.toLong().toString() else "%.1f".format(count)

/** The amber of the zones served to, from the fewest serves (`share`
 * near 0) to the most (1). */
internal fun densityColor(share: Double): Color =
    lerp(Scout4AllColors.surfaceVariant, Scout4AllColors.primary, (0.25 + 0.75 * share.coerceIn(0.0, 1.0)).toFloat())

/** The text on a background: white or black, whichever reads better (at
 * 4.5:1 at least on any color). */
internal fun readableOn(background: Color): Color =
    listOf(Color.White, Color.Black).maxBy { contrast(it, background) }

internal fun contrast(a: Color, b: Color): Double {
    val (light, dark) = listOf(a.luminance(), b.luminance()).sortedDescending()
    return (light + 0.05) / (dark + 0.05)
}

/** A zone: its name, its value and a detail under it, on its color if it
 * has one; a conflict zone, narrow, its name and value only. TalkBack reads
 * them all. */
@Composable
private fun ZoneCell(t: TargetStats, background: Color?, value: String, detail: String) =
    ZoneCell(t.label, t.label, t.zone, background, value, detail)

@Composable
private fun ZoneCell(spoken: String, label: String, zone: Boolean, background: Color?, value: String, detail: String) {
    val text = background?.let { readableOn(it) } ?: MaterialTheme.colorScheme.onSurfaceVariant
    val lines = when {
        !zone -> listOf(label to MaterialTheme.typography.labelSmall, value to MaterialTheme.typography.labelMedium)
            .filter { it.first.isNotEmpty() }
        else -> listOf(
            label to MaterialTheme.typography.labelSmall,
            value to MaterialTheme.typography.labelLarge,
            detail to MaterialTheme.typography.labelSmall,
        ).filter { it.first.isNotEmpty() }
    }
    Surface(
        Modifier.fillMaxSize().padding(1.dp).clearAndSetSemantics {
            contentDescription = listOf(spoken, value, detail).filter { it.isNotEmpty() }.joinToString(" ")
        },
        shape = MaterialTheme.shapes.small,
        color = background ?: MaterialTheme.colorScheme.surfaceVariant.copy(alpha = if (zone) 1f else 0.5f),
    ) {
        Column(verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
            lines.forEach { (line, style) ->
                Text(
                    line,
                    style = style,
                    fontWeight = if (style == MaterialTheme.typography.labelLarge) FontWeight.Bold else null,
                    color = text,
                    maxLines = 1,
                    softWrap = false,
                )
            }
        }
    }
}

/** The court: the opponent's as our server sees it (their end line at the
 * top, 1 on the left) or ours from the bench (the net at the top, 4 on the
 * left), the zones a row each as `rows` gives them: three zones, or with
 * the conflict zones narrower between them. A row as tall as a zone is
 * wide, more with its texts at a large font. */
@Composable
private fun <T> ZoneGrid(rows: List<List<T>>, cell: @Composable (T) -> Unit) =
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        // the conflict zones a sixth of the court, the zones what's left
        val conflict = maxWidth / 6
        val zone = (maxWidth - conflict * 2) / 3
        val fives = listOf(zone, conflict, zone, conflict, zone)
        val threes = List(3) { maxWidth / 3 }
        val height = if (rows.any { it.size == 5 }) zone else maxWidth / 3
        Column(Modifier.fillMaxWidth()) {
            rows.forEach { row ->
                Row(Modifier.fillMaxWidth().height(IntrinsicSize.Min).heightIn(min = height)) {
                    val widths = if (row.size == 5) fives else threes
                    row.forEachIndexed { c, t ->
                        Box(Modifier.width(widths.getOrElse(c) { 0.dp }).fillMaxHeight()) { cell(t) }
                    }
                }
            }
        }
    }
