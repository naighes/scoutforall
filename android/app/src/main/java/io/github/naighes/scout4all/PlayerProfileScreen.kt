package io.github.naighes.scout4all

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.Player
import io.github.naighes.scout4all.core.PlayerProfileStats
import io.github.naighes.scout4all.core.TeamDetails
import kotlinx.coroutines.launch
import java.time.LocalDate
import java.time.Period
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

private data class ProfileData(val team: TeamDetails, val player: Player, val stats: PlayerProfileStats)

/** Read only profile for one player in one team season. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PlayerProfileScreen(
    teamId: String,
    playerId: String,
    onBack: () -> Unit,
    onEdit: () -> Unit,
    onTeam: (String) -> Unit,
    onDeleted: () -> Unit,
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    var data by remember(teamId, playerId) { mutableStateOf<ProfileData?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var loading by remember { mutableStateOf(true) }
    var tab by rememberSaveable { mutableIntStateOf(0) }
    var asking by rememberSaveable { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(teamId, playerId) {
        loading = true
        try {
            data = app.core { core ->
                val team = core.team(teamId)
                val player = team.players.first { it.id == playerId }
                ProfileData(team, player, core.playerProfileStats(teamId, playerId))
            }
            error = null
        } catch (e: MobileException) { error = e.reason }
        catch (e: NoSuchElementException) { error = e.message }
        loading = false
    }
    if (asking) {
        val name = data?.player?.name.orEmpty()
        DeletionDialog(
            stringResource(R.string.delete_player_text, name),
            onConfirm = {
                asking = false
                scope.launch {
                    try { app.core { it.removePlayer(teamId, playerId) }; onDeleted() }
                    catch (e: MobileException) { error = e.reason }
                }
            }, onDismiss = { asking = false }, confirm = stringResource(R.string.remove_player),
        )
    }
    BackHandler(onBack = onBack)
    Scaffold(topBar = {
        TopAppBar(
            title = { Text(data?.team?.name.orEmpty(), maxLines = 1, overflow = TextOverflow.Ellipsis) },
            navigationIcon = { BackButton(onBack) },
            actions = {
                if (data != null) {
                    IconButton(onClick = onEdit) { Icon(Icons.Filled.Edit, stringResource(R.string.edit_player)) }
                    IconButton(onClick = { asking = true }) { Icon(Icons.Filled.Delete, stringResource(R.string.remove_player)) }
                }
            },
        )
    }) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        when {
            loading -> Centered(modifier) { CircularProgressIndicator() }
            error != null -> Centered(modifier) { Text(error.orEmpty()) }
            data == null -> Centered(modifier) { Text(stringResource(R.string.no_players)) }
            else -> {
                val d = data!!
                Column(modifier) {
                    Row(
                        Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.primaryContainer).padding(20.dp),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(18.dp),
                    ) {
                        ProfileAvatar(d.player.id, d.player.name, 104.dp)
                        Column(Modifier.weight(1f)) {
                            Text(d.player.number.toString().padStart(2, '0'), style = MaterialTheme.typography.displayMedium, fontWeight = FontWeight.Bold, color = MaterialTheme.colorScheme.primary)
                            Text(d.player.name, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
                            Text(listOfNotNull(d.player.role, "${d.team.name} ${d.team.year}").joinToString(" · "), style = MaterialTheme.typography.bodyMedium)
                        }
                        TeamLogo(teamId, 44.dp)
                    }
                    PrimaryTabRow(selectedTabIndex = tab) {
                        Tab(tab == 0, { tab = 0 }, text = { Text(stringResource(R.string.profile)) })
                        Tab(tab == 1, { tab = 1 }, text = { Text(stringResource(R.string.statistics)) })
                    }
                    if (tab == 0) ProfileDetails(d, onTeam, Modifier.fillMaxSize())
                    else LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(20.dp)) {
                        item { Text(stringResource(R.string.statistics), style = MaterialTheme.typography.titleLarge) }
                        item { Text(stringResource(R.string.player_statistics_coming), Modifier.padding(top = 8.dp), color = MaterialTheme.colorScheme.onSurfaceVariant) }
                    }
                }
            }
        }
    }
}

@Composable
private fun ProfileAvatar(playerId: String, name: String, size: androidx.compose.ui.unit.Dp) {
    val photo = photoOf(playerId)
    Box(
        Modifier.size(size).clip(RoundedCornerShape(18.dp)).background(MaterialTheme.colorScheme.surfaceVariant),
        contentAlignment = Alignment.Center,
    ) {
        if (photo != null) Image(photo, contentDescription = null, Modifier.fillMaxSize(), contentScale = androidx.compose.ui.layout.ContentScale.Fit)
        else Text(name.split(Regex("\\s+")).take(2).mapNotNull { it.firstOrNull() }.joinToString("").uppercase(), style = MaterialTheme.typography.headlineMedium, color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold)
    }
}

@Composable
private fun ProfileDetails(d: ProfileData, onTeam: (String) -> Unit, modifier: Modifier) {
    val f = d.player.form
    val s = d.stats
    LazyColumn(modifier, contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        item {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (s.matchesPlayed == 0u) {
                    Text(stringResource(R.string.no_matches_played), Modifier.weight(3f).align(Alignment.CenterVertically), color = MaterialTheme.colorScheme.onSurfaceVariant)
                } else {
                    ProfileMetric(stringResource(R.string.matches), "${s.matchesPlayed} / ${s.matchesScouted}", Modifier.weight(1f))
                    ProfileMetric(stringResource(R.string.sets), s.setsPlayed.toString(), Modifier.weight(1f))
                    ProfileMetric(
                        stringResource(R.string.points), s.points.toString(), Modifier.weight(1f),
                        stringResource(R.string.player_points_detail, s.attackPoints.toInt(), s.blockPoints.toInt(), s.servePoints.toInt()),
                    )
                }
                if (s.trainingSessions > 0u) ProfileMetric(stringResource(R.string.attendance), s.attendancePercent?.let { "$it%" } ?: "—", Modifier.weight(1f))
            }
        }
        item {
            val birth = runCatching { LocalDate.parse(f.birthDate) }.getOrNull()
            val age = birth?.let { Period.between(it, LocalDate.now()).years }
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                birth?.let { ProfileField(stringResource(R.string.birth_date), "${it.format(DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM))}${age?.let { years -> " ($years)" }.orEmpty()}") }
                f.nationality?.let { code ->
                    var country by remember(code) { mutableStateOf("") }
                    val app = LocalContext.current.applicationContext as Scout4AllApplication
                    LaunchedEffect(code) { country = runCatching { app.core { it.countryName(code) } }.getOrDefault("") }
                    if (country.isNotEmpty()) ProfileField(stringResource(R.string.nationality), country)
                }
                f.heightCm.takeIf { it.isNotBlank() }?.let { ProfileField(stringResource(R.string.height), "$it ${stringResource(R.string.centimetres)}") }
                f.dominantHand?.let { ProfileField(stringResource(R.string.dominant_hand), stringResource(if (it.name == "RIGHT") R.string.right_hand else R.string.left_hand)) }
                f.federationId.takeIf { it.isNotBlank() }?.let { ProfileField(stringResource(R.string.federation_id), it) }
            }
        }
        if (f.notes.isNotBlank()) item { ProfileField(stringResource(R.string.notes), f.notes) }
        if (s.otherTeams.isNotEmpty()) item {
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(stringResource(R.string.also_plays_in), style = MaterialTheme.typography.titleMedium)
                s.otherTeams.forEach { team ->
                    TextButton(onClick = { onTeam(team.id) }) { Text("${team.number} · ${team.name} ${team.year}") }
                }
            }
        }
    }
}

@Composable
private fun ProfileMetric(label: String, value: String, modifier: Modifier = Modifier, detail: String? = null) {
    Column(modifier.background(MaterialTheme.colorScheme.surfaceVariant, RoundedCornerShape(14.dp)).padding(12.dp)) {
        Text(label.uppercase(), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary)
        Text(value, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
        detail?.let { Text(it, style = MaterialTheme.typography.labelSmall) }
    }
}

@Composable
private fun ProfileField(label: String, value: String) {
    Column {
        Text(label.uppercase(), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary)
        Text(value, style = MaterialTheme.typography.bodyLarge)
    }
}
