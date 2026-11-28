package io.github.naighes.scout4all

import android.content.Intent
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material3.*
import androidx.compose.material3.Icon
import androidx.compose.material3.LocalContentColor
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.naighes.scout4all.core.Contacts
import io.github.naighes.scout4all.core.DominantHand
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.Player
import io.github.naighes.scout4all.core.PlayerProfile
import io.github.naighes.scout4all.core.Scout4All
import io.github.naighes.scout4all.core.TeamDetails
import java.time.LocalDate
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import kotlinx.coroutines.launch

private data class ProfileData(
    val team: TeamDetails,
    val player: Player,
    val stats: PlayerProfile,
    /** the nationality by name */
    val country: String?,
    /** the other teams the player plays in, by name */
    val otherTeams: List<String>,
)

private sealed interface ProfileState {
    data object Loading : ProfileState
    data class Loaded(val data: ProfileData) : ProfileState
    data class Failed(val message: String) : ProfileState
}

/** Null when the player isn't in the team any more. */
private fun Scout4All.profileData(teamId: String, playerId: String): ProfileData? {
    val team = team(teamId)
    return team.players.firstOrNull { it.id == playerId }?.let { player ->
        ProfileData(
            team,
            player,
            playerProfile(teamId, playerId),
            player.form.nationality?.let { countryName(it) },
            otherTeams(teamId, playerId),
        )
    }
}

/** Tags of the band of the header and of the face on it (tests). */
const val PROFILE_BAND = "profile_band"
const val PROFILE_FACE = "profile_face"

private val BAND = 220.dp
private val FACE = 214.dp
private val ITEM_COLUMNS = 2

/** Read only profile for one player in one team season. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun PlayerProfileScreen(
    teamId: String,
    playerId: String,
    onBack: () -> Unit,
    onEdit: () -> Unit,
    onTeam: (String) -> Unit,
    onDeleted: () -> Unit,
    onDetail: (LockedAnalysis) -> Unit = {},
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    var state by remember(teamId, playerId) { mutableStateOf<ProfileState>(ProfileState.Loading) }
    var tab by rememberSaveable { mutableIntStateOf(0) }
    val statistics = playerStatisticsState(teamId, playerId, shown = tab == 1)
    var asking by rememberSaveable { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(teamId, playerId) {
        state = try {
            app.core { it.profileData(teamId, playerId) }
                ?.let { ProfileState.Loaded(it) }
                ?: ProfileState.Failed(app.getString(R.string.no_players))
        } catch (e: MobileException) {
            ProfileState.Failed(e.reason)
        }
    }
    val data = (state as? ProfileState.Loaded)?.data
    val remove: (Scout4All.() -> Unit) -> Unit = { removal ->
        asking = false
        scope.launch {
            try {
                app.core { it.removal() }
                onDeleted()
            } catch (e: MobileException) {
                state = ProfileState.Failed(e.reason)
            }
        }
    }
    if (asking && data != null) {
        PlayerRemovalDialog(
            data.player.name, data.player.number.toInt(), data.otherTeams,
            onRemove = { remove { removePlayer(teamId, playerId) } },
            onDelete = { remove { deletePerson(playerId) } },
            onDismiss = { asking = false },
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
        when (val s = state) {
            ProfileState.Loading -> Centered(modifier) { CircularProgressIndicator() }
            is ProfileState.Failed -> LoadFailed(s.message, modifier)
            // the header scrolls too: a phone on its side is short
            is ProfileState.Loaded -> LazyColumn(modifier) {
                item { ProfileHeader(s.data) }
                item {
                    PrimaryTabRow(selectedTabIndex = tab) {
                        Tab(tab == 0, { tab = 0 }, text = { Text(stringResource(R.string.profile)) })
                        Tab(tab == 1, { tab = 1 }, text = { Text(stringResource(R.string.statistics)) })
                    }
                }
                item {
                    when (tab) {
                        0 -> ProfileDetails(s.data, onTeam)
                        else -> PlayerStatisticsTab(statistics) {
                            onDetail(LockedAnalysis(playerId, statistics.choice, statistics.competition))
                        }
                    }
                }
            }
        }
    }
}

/** The band with the logo and the face, which touches the right edge (the
 * photo cuts the shoulders at its sides); under it, past the line of the
 * accent, number, name, role and team. */
@Composable
private fun ProfileHeader(d: ProfileData) {
    val accent = MaterialTheme.colorScheme.primary
    val photo = photoOf(d.player.id)
    Column(Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceVariant)) {
        Box(Modifier.fillMaxWidth().height(BAND).testTag(PROFILE_BAND)) {
            TeamLogo(d.team.id, 54.dp, Modifier.align(Alignment.TopStart).padding(16.dp))
            Box(Modifier.align(Alignment.BottomEnd).size(FACE).testTag(PROFILE_FACE), contentAlignment = Alignment.Center) {
                when (photo) {
                    null -> Text(
                        d.player.initials,
                        style = MaterialTheme.typography.displayLarge,
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    else -> Image(photo, contentDescription = null, Modifier.fillMaxSize(), contentScale = ContentScale.Fit)
                }
            }
        }
        HorizontalDivider(thickness = 4.dp, color = accent)
        Row(
            Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Text(
                d.player.number.toString().padStart(2, '0'),
                style = MaterialTheme.typography.displayLarge,
                fontWeight = FontWeight.Bold,
                color = accent,
            )
            Column(Modifier.weight(1f)) {
                Text(
                    d.player.name.uppercase(),
                    style = MaterialTheme.typography.headlineSmall,
                    fontWeight = FontWeight.Bold,
                    fontStyle = FontStyle.Italic,
                )
                Text(
                    listOfNotNull(d.player.role, "${d.team.name} ${d.team.year}").joinToString(" · "),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

/** A value under its small label. */
private data class ProfileItem(val label: String, val value: String, val detail: String? = null)

/** Matches, sets and points once the player has been on court, and the
 * attendance when the team trains. */
@Composable
private fun numbers(s: PlayerProfile): List<ProfileItem> {
    val played = when (s.matchesPlayed) {
        0u -> emptyList()
        else -> listOf(
            ProfileItem(
                stringResource(R.string.matches),
                s.matchesPlayed.toString(),
                stringResource(R.string.player_out_of, s.matchesScouted.toInt()),
            ),
            ProfileItem(stringResource(R.string.sets), s.setsPlayed.toString()),
            ProfileItem(
                stringResource(R.string.points),
                s.points.toString(),
                stringResource(R.string.player_points_detail, s.attackPoints.toInt(), s.blockPoints.toInt(), s.servePoints.toInt()),
            ),
        )
    }
    val attendance = ProfileItem(stringResource(R.string.attendance), s.attendancePercent?.let { "$it%" } ?: "—")
    return played + listOfNotNull(attendance.takeIf { s.hasTraining })
}

/** What's known of the person, the empty ones left out. */
@Composable
private fun fields(d: ProfileData): List<ProfileItem> {
    val form = d.player.form
    val birth = runCatching { LocalDate.parse(form.birthDate) }.getOrNull()?.let { day ->
        val shown = day.format(DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM))
        shown + d.stats.age?.let { " ($it)" }.orEmpty()
    }
    val hand = form.dominantHand?.let {
        stringResource(
            when (it) {
                DominantHand.RIGHT -> R.string.right_hand
                DominantHand.LEFT -> R.string.left_hand
            },
        )
    }
    val centimetres = stringResource(R.string.centimetres)
    return listOf(
        R.string.birth_date to birth,
        R.string.birth_place to form.birthPlace.takeIf { it.isNotBlank() },
        R.string.nationality to d.country,
        R.string.height to form.heightCm.takeIf { it.isNotBlank() }?.let { "$it $centimetres" },
        R.string.dominant_hand to hand,
        R.string.federation_id to form.federationId.takeIf { it.isNotBlank() },
    ).mapNotNull { (label, value) -> value?.let { ProfileItem(stringResource(label), it) } }
}

@Composable
private fun ProfileDetails(d: ProfileData, onTeam: (String) -> Unit) {
    val s = d.stats
    val numbers = numbers(s)
    val fields = fields(d)
    val notes = d.player.form.notes.takeIf { it.isNotBlank() }
    val contacts = d.player.contacts
    val reachable = contacts.email != null || contacts.phone != null
    Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(18.dp)) {
        if (s.matchesPlayed == 0u) {
            Text(stringResource(R.string.no_matches_played), color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        ProfileGrid(numbers, 30.sp)
        if (fields.isNotEmpty() || reachable || notes != null || s.otherTeams.isNotEmpty()) HorizontalDivider()
        ProfileGrid(fields, 18.sp)
        ContactLines(contacts)
        notes?.let { ProfileValue(ProfileItem(stringResource(R.string.notes), it), 18.sp) }
        if (s.otherTeams.isNotEmpty()) {
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                ProfileLabel(stringResource(R.string.also_plays_in))
                s.otherTeams.forEach { team ->
                    OutlinedButton(onClick = { onTeam(team.id) }) { Text("${team.number} · ${team.name} ${team.year}") }
                }
            }
        }
    }
}

/** The email and the phone, each with what can be done with it: another
 * app writes, calls or opens the conversation. */
@Composable
private fun ContactLines(contacts: Contacts) {
    val context = LocalContext.current
    val opened = noAppDialog()
    val to = { action: String, link: String -> { opened(context.open(Intent(action, Uri.parse(link)))) } }
    contacts.email?.let { email ->
        ContactLine(
            ProfileItem(stringResource(R.string.email), email.address),
            listOf(
                ContactAction(R.drawable.ic_mail, stringResource(R.string.write_email), to(Intent.ACTION_SENDTO, email.link)),
            ),
        )
    }
    contacts.phone?.let { phone ->
        ContactLine(
            ProfileItem(stringResource(R.string.phone), phone.shown),
            listOfNotNull(
                // the dialer, which needs no permission
                phone.callLink?.let {
                    ContactAction(R.drawable.ic_call, stringResource(R.string.call), to(Intent.ACTION_DIAL, it))
                },
                phone.whatsappLink?.let {
                    // the logo keeps its colors
                    ContactAction(R.drawable.ic_whatsapp, "WhatsApp", to(Intent.ACTION_VIEW, it), tinted = false)
                },
            ),
        )
    }
}

/** What can be done with a contact: its icon, [tinted] as the words of
 * the button unless in its own colors. */
private data class ContactAction(
    val icon: Int,
    val label: String,
    val act: () -> Unit,
    val tinted: Boolean = true,
)

@Composable
private fun ContactLine(item: ProfileItem, actions: List<ContactAction>) =
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        ProfileValue(item, 18.sp)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            actions.forEach { action ->
                OutlinedButton(onClick = action.act) {
                    Icon(
                        painterResource(action.icon),
                        contentDescription = null,
                        modifier = Modifier.padding(end = 8.dp).size(18.dp).testTag("contact-icon"),
                        tint = if (action.tinted) LocalContentColor.current else Color.Unspecified,
                    )
                    Text(action.label)
                }
            }
        }
    }

/** The items in columns of the same width, row by row. */
@Composable
private fun ProfileGrid(items: List<ProfileItem>, size: androidx.compose.ui.unit.TextUnit) {
    if (items.isEmpty()) return
    Column(verticalArrangement = Arrangement.spacedBy(18.dp)) {
        items.chunked(ITEM_COLUMNS).forEach { row ->
            Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                row.forEach { ProfileValue(it, size, Modifier.weight(1f)) }
                repeat(ITEM_COLUMNS - row.size) { Spacer(Modifier.weight(1f)) }
            }
        }
    }
}

@Composable
private fun ProfileLabel(label: String) = Text(
    label.uppercase(),
    style = MaterialTheme.typography.labelMedium,
    fontWeight = FontWeight.Bold,
    fontStyle = FontStyle.Italic,
    color = MaterialTheme.colorScheme.primary,
)

@Composable
private fun ProfileValue(item: ProfileItem, size: androidx.compose.ui.unit.TextUnit, modifier: Modifier = Modifier) =
    Column(modifier, verticalArrangement = Arrangement.spacedBy(2.dp)) {
        ProfileLabel(item.label)
        Text(item.value, fontSize = size, lineHeight = size * 1.2f, fontWeight = FontWeight.Bold)
        item.detail?.let {
            Text(it, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
