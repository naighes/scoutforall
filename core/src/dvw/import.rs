//! The team of a side of the file among ours: the similar teams to propose,
//! and the plan of the import once the user has chosen one, or a new team.
//! Nothing is merged without the user: the similar teams say why they are.

use crate::{
    analytics::finished::FinishedMatches,
    constants::{DEFAULT_MAX_SUBSTITUTIONS, LEGACY_MAX_SUBSTITUTIONS},
    dvw::{
        model::{DvwFile, DvwRole, MatchDate, Player, Side},
        translate::{
            Issue, Mappings, TranslateError, Untranslatable, Us, has_video_clock, translate_set,
        },
    },
    errors::{AppError, MatchError},
    localization::Labels,
    match_form::MatchInput,
    providers::{
        match_reader::MatchReader,
        match_writer::MatchWriter,
        set_writer::SetWriter,
        team_writer::{TeamInput, TeamWriter},
    },
    shapes::{
        country::CountryCode,
        enums::{FriendlyName, GenderEnum, RoleEnum, TeamClassificationEnum},
        r#match::{MatchEntry, MatchSource},
        player::PlayerEntry,
        refusal::Refusal,
        set::ReplayIssue,
        team::TeamEntry,
    },
    team_form::TeamFormError,
};
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveTime};
use std::collections::HashMap;
use thiserror::Error;
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

/// Why the team of a side can't be imported.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TeamIssue {
    /// Neither a season nor a date telling it.
    #[error("the season of the match can't be told")]
    NoYear,
    /// A new team needs what the form of a team needs: its level, gender
    /// and country, in this order.
    #[error("the new team: {0:?}")]
    Incomplete(TeamFormError),
    /// A player of the team with the number of a player of the file and
    /// another surname.
    #[error("number {number}: {file} in the file, {team} in the team")]
    PlayerConflict {
        number: u8,
        file: String,
        team: String,
    },
}

/// A team of ours like the one of the file, and why: the same season, and
/// the same name or at least half of the players of the file (by number
/// and surname).
#[derive(Debug, Clone)]
pub struct SimilarTeam<'a> {
    pub team: &'a TeamEntry,
    pub same_name: bool,
    /// The players of the file found in the team.
    pub same_players: usize,
    /// The players of the file.
    pub players: usize,
}

/// The team the side is imported into, as the user chose it.
#[derive(Debug, Clone)]
pub enum TeamChoice<'a> {
    Existing(&'a TeamEntry),
    /// A new team with the name of the file; the file tells neither its
    /// level, nor its gender, nor its country: the user does, as in the
    /// form of a team.
    New {
        classification: Option<TeamClassificationEnum>,
        gender: Option<GenderEnum>,
        country: Option<CountryCode>,
    },
}

/// The team with the players of the file, ready to be saved.
#[derive(Debug, Clone)]
pub struct TeamPlan {
    /// The chosen team with the new players, or the new team.
    pub team: TeamEntry,
    pub new_team: bool,
    /// The players added to the team.
    pub new_players: Vec<Uuid>,
    /// The players of the file, by number: the roster of the match.
    pub roster: Vec<Uuid>,
    /// The number of each player of the file.
    pub numbers: HashMap<u8, Uuid>,
    /// The numbers of the liberos of the file.
    pub liberos: Vec<u8>,
}

/// Lowercase, without accents and punctuation, the spaces compacted: how
/// names are compared.
pub fn normalized(text: &str) -> String {
    text.nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .map(|c| match c.is_alphanumeric() {
            true => c,
            false => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The year of the season, as our teams have it: the first one written in
/// the season (`2014/2015` → 2014), otherwise from the date (from July the
/// year itself, before it the one before). None when neither tells it.
pub fn season_year(file: &DvwFile) -> Option<u16> {
    let written = file.match_info.season.as_deref().and_then(first_year);
    let dated = match file.match_info.date {
        MatchDate::Known(d) => Some(year_of(d)),
        MatchDate::Ambiguous(a, b) if year_of(a) == year_of(b) => Some(year_of(a)),
        MatchDate::Ambiguous(_, _) | MatchDate::Missing => None,
    };
    written.or(dated)
}

// a run of four digits that is a plausible year
fn first_year(season: &str) -> Option<u16> {
    season
        .split(|c: char| !c.is_ascii_digit())
        .filter(|digits| digits.len() == 4)
        .filter_map(|digits| digits.parse::<u16>().ok())
        .find(|year| (1950..=2200).contains(year))
}

fn year_of(date: NaiveDate) -> u16 {
    let year = u16::try_from(date.year()).unwrap_or_default();
    match date.month() {
        7..=12 => year,
        _ => year.saturating_sub(1),
    }
}

/// Our teams like the one of the side, the most alike first: the same
/// season, and the same name or at least half of the players.
pub fn similar_teams<'a>(
    file: &DvwFile,
    side: Side,
    teams: &'a [TeamEntry],
) -> Vec<SimilarTeam<'a>> {
    let year = season_year(file);
    let mut similar: Vec<SimilarTeam<'a>> = teams
        .iter()
        .filter(|team| Some(team.year) == year)
        .map(|team| similarity(file, side, team))
        .filter(|s| s.same_name || (s.players > 0 && s.same_players * 2 >= s.players))
        .collect();
    similar.sort_by_key(|s| (!s.same_name, std::cmp::Reverse(s.same_players)));
    similar
}

/// How much `team` is like the one of the side: the name, the players.
pub fn similarity<'a>(file: &DvwFile, side: Side, team: &'a TeamEntry) -> SimilarTeam<'a> {
    let name = normalized(&file.team(side).name);
    let players = file.players(side);
    SimilarTeam {
        team,
        same_name: !name.is_empty() && normalized(&team.name) == name,
        same_players: players
            .iter()
            .filter(|p| active_with_number(team, p.number).is_some_and(|t| same(t, p)))
            .count(),
        players: players.len(),
    }
}

/// The team with the players of the side: those of the team by number and
/// surname, the others added. A player of the team with the number and
/// another surname stops the import.
pub fn team_plan(file: &DvwFile, side: Side, choice: TeamChoice) -> Result<TeamPlan, TeamIssue> {
    let (mut team, new_team) = match choice {
        TeamChoice::Existing(team) => (team.clone(), false),
        TeamChoice::New {
            classification,
            gender,
            country,
        } => (
            TeamEntry {
                name: file.team(side).name.trim().to_string(),
                year: season_year(file).ok_or(TeamIssue::NoYear)?,
                players: vec![],
                id: Uuid::new_v4(),
                classification: Some(
                    classification
                        .ok_or(TeamIssue::Incomplete(TeamFormError::MissingClassification))?,
                ),
                gender: Some(gender.ok_or(TeamIssue::Incomplete(TeamFormError::MissingGender))?),
                country: Some(country.ok_or(TeamIssue::Incomplete(TeamFormError::MissingCountry))?),
            },
            true,
        ),
    };
    let mut file_players: Vec<&Player> = file.players(side).iter().collect();
    file_players.sort_by_key(|p| p.number);
    let mut new_players = vec![];
    let mut numbers = HashMap::new();
    for p in file_players {
        let id = match (
            active_with_number(&team, p.number),
            removed_with_number(&team, p),
        ) {
            (Some(t), _) if same(t, p) => t.id,
            (Some(t), _) => {
                return Err(TeamIssue::PlayerConflict {
                    number: p.number,
                    file: full_name(p),
                    team: t.name.clone(),
                });
            }
            // removed since: the same player, still there for the match
            (None, Some(t)) => t.id,
            (None, None) => {
                let player = PlayerEntry {
                    name: full_name(p),
                    role: role(p),
                    number: p.number,
                    ..PlayerEntry::default()
                };
                new_players.push(player.id);
                let id = player.id;
                team.players.push(player);
                id
            }
        };
        numbers.insert(p.number, id);
    }
    let mut by_number: Vec<(u8, Uuid)> = numbers.iter().map(|(n, id)| (*n, *id)).collect();
    by_number.sort_by_key(|(n, _)| *n);
    let liberos = file
        .players(side)
        .iter()
        .filter(|p| p.libero || p.role == Some(DvwRole::Libero))
        .map(|p| p.number)
        .collect();
    Ok(TeamPlan {
        team,
        new_team,
        new_players,
        roster: by_number.into_iter().map(|(_, id)| id).collect(),
        numbers,
        liberos,
    })
}

fn active_with_number(team: &TeamEntry, number: u8) -> Option<&PlayerEntry> {
    team.players
        .iter()
        .find(|t| !t.deleted && t.number == number)
}

fn removed_with_number<'a>(team: &'a TeamEntry, p: &Player) -> Option<&'a PlayerEntry> {
    team.players
        .iter()
        .find(|t| t.deleted && t.number == p.number && same(t, p))
}

// the surname of the file at the end of the name of ours ("Nome Cognome")
fn same(ours: &PlayerEntry, p: &Player) -> bool {
    let name = normalized(&ours.name);
    let surname = normalized(&p.surname);
    !surname.is_empty() && (name == surname || name.ends_with(&format!(" {surname}")))
}

/// "Nome Cognome", a part written in capitals (the surname, as DataVolley
/// writes it, or the name, as some scouts do) as a name.
fn full_name(p: &Player) -> String {
    [as_name(&p.name), as_name(&p.surname)]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn as_name(text: &str) -> String {
    match text.chars().any(char::is_lowercase) {
        true => text.trim().to_string(),
        false => capitalized(text.trim()),
    }
}

// each word capitalized, also after a hyphen or an apostrophe
fn capitalized(text: &str) -> String {
    let mut first = true;
    text.chars()
        .flat_map(|c| {
            let out: Vec<char> = match first {
                true => c.to_uppercase().collect(),
                false => c.to_lowercase().collect(),
            };
            first = !c.is_alphanumeric();
            out
        })
        .collect()
}

fn role(p: &Player) -> Option<RoleEnum> {
    match (p.libero, p.role) {
        (true, _) | (_, Some(DvwRole::Libero)) => Some(RoleEnum::Libero),
        (_, Some(DvwRole::OutsideHitter)) => Some(RoleEnum::OutsideHitter),
        (_, Some(DvwRole::Opposite)) => Some(RoleEnum::OppositeHitter),
        (_, Some(DvwRole::MiddleBlocker)) => Some(RoleEnum::MiddleBlocker),
        (_, Some(DvwRole::Setter)) => Some(RoleEnum::Setter),
        (false, None) => None,
    }
}

/// The substitutions per set proposed for a match of the day: six before
/// the rules of 2026, then eight; the user may change them.
pub fn default_max_substitutions(day: Option<NaiveDate>) -> u8 {
    match day.map(|d| d.year()) {
        Some(year) if year < 2026 => LEGACY_MAX_SUBSTITUTIONS,
        _ => DEFAULT_MAX_SUBSTITUTIONS,
    }
}

/// Why the match of a side can't be imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchIssue {
    /// The file has no set.
    NoSets,
    /// A set doesn't translate exactly.
    Set(TranslateError),
    /// The final score of a set (us, them): the one of `[3SET]` and ours.
    SetScore {
        set_number: u8,
        file: (u8, u8),
        ours: (u8, u8),
    },
    /// The sets of the file don't finish the match.
    Unfinished,
    /// A translated set our version doesn't replay.
    Replay(ReplayIssue),
}

/// The match of the side for the team of the plan, ready to be saved, and
/// the mappings used by its sets, for the summary.
#[derive(Debug, Clone)]
pub struct MatchPlan {
    pub entry: MatchEntry,
    pub mappings: Mappings,
}

/// When the match was played: the day (chosen by the user when the file
/// can't tell it), at the time of the file or midnight, in `offset`.
pub fn match_date(file: &DvwFile, day: NaiveDate, offset: FixedOffset) -> DateTime<FixedOffset> {
    let time = file.match_info.time.unwrap_or(NaiveTime::MIN);
    day.and_time(time)
        .and_local_timezone(offset)
        .single()
        .unwrap_or_else(|| day.and_time(NaiveTime::MIN).and_utc().fixed_offset())
}

/// The match of the side: every set of the file translated exactly, with
/// its final score as `[3SET]` writes it, and the match finished; against
/// the other team of the file.
pub fn match_plan(
    file: &DvwFile,
    side: Side,
    team: &TeamPlan,
    date: DateTime<FixedOffset>,
    max_substitutions: u8,
) -> Result<MatchPlan, MatchIssue> {
    let us = Us {
        side,
        players: &team.numbers,
        liberos: &team.liberos,
    };
    let other = match side {
        Side::Home => Side::Visiting,
        Side::Visiting => Side::Home,
    };
    let opponent = file.team(other).name.trim().to_string();
    let mut sets = vec![];
    let mut mappings = Mappings::new();
    for result in &file.sets {
        let (set, used) = translate_set(file, &us, result.set_number, date, max_substitutions)
            .map_err(MatchIssue::Set)?;
        let snapshot = set.compute_snapshot(max_substitutions).map_err(|e| {
            MatchIssue::Set(TranslateError {
                set_number: result.set_number,
                line: None,
                issue: Issue::Event(e.to_string()),
            })
        })?;
        let ours = (snapshot.score_us, snapshot.score_them);
        let written = match side {
            Side::Home => result.final_score,
            Side::Visiting => (result.final_score.1, result.final_score.0),
        };
        if ours != written {
            return Err(MatchIssue::SetScore {
                set_number: result.set_number,
                file: written,
                ours,
            });
        }
        for (name, count) in used {
            *mappings.entry(name).or_default() += count;
        }
        sets.push(set);
    }
    if sets.is_empty() {
        return Err(MatchIssue::NoSets);
    }
    let entry = MatchEntry {
        id: MatchEntry::id_of(&opponent, &date),
        opponent,
        date,
        team: team.team.with_roster(&team.roster),
        home: side == Side::Home,
        max_substitutions,
        notes: String::new(),
        roster: team.roster.clone(),
        sets,
        source: MatchSource::Dvw,
        video_clock: has_video_clock(file),
    };
    if let Some(issue) = entry.replay_issue() {
        return Err(MatchIssue::Replay(issue));
    }
    match entry.get_status() {
        Ok(status) if status.match_finished => Ok(MatchPlan { entry, mappings }),
        _ => Err(MatchIssue::Unfinished),
    }
}

/// Whether the team has the match already: the import overwrites it only
/// when the user confirms.
pub async fn match_exists(
    plan: &MatchPlan,
    match_reader: &(impl MatchReader + Sync + ?Sized),
) -> Result<bool, AppError> {
    saved_as(plan, match_reader).await.map(|id| id.is_some())
}

/// The id of the match saved already that the import tells again, if any
/// (`MatchReader::saved_as`).
async fn saved_as(
    plan: &MatchPlan,
    match_reader: &(impl MatchReader + Sync + ?Sized),
) -> Result<Option<String>, AppError> {
    match_reader
        .saved_as(&plan.entry.team, &plan.entry.id)
        .await
}

/// Saves the import: the team with its new players, then the match and
/// its sets, in place of the one saved when `overwrite` (its sets beyond
/// the imported ones removed), and the match queued for the statistics as
/// any finished match. A match saved and not to overwrite: nothing saved.
#[allow(clippy::too_many_arguments)]
pub async fn save_import(
    team: &TeamPlan,
    plan: &MatchPlan,
    overwrite: bool,
    team_writer: &(impl TeamWriter + Sync + ?Sized),
    match_reader: &(impl MatchReader + Sync + ?Sized),
    match_writer: &(impl MatchWriter + Sync + ?Sized),
    set_writer: &(impl SetWriter + Sync + ?Sized),
    finished: &(impl FinishedMatches + ?Sized),
) -> Result<MatchEntry, AppError> {
    let saved = saved_as(plan, match_reader).await?;
    if saved.is_some() && !overwrite {
        return Err(AppError::Match(MatchError::MatchAlreadyExists(
            plan.entry.id.clone(),
        )));
    }
    let saved_team = team_writer
        .save(TeamInput::Existing(team.team.clone()))
        .await?;
    let m = match saved {
        Some(id) => {
            let entry = &MatchEntry {
                id,
                ..plan.entry.clone()
            };
            match_writer.replace_details(entry).await?;
            MatchEntry {
                sets: vec![],
                ..entry.clone()
            }
        }
        None => {
            let entry = &plan.entry;
            let created = match_writer
                .create(
                    &saved_team,
                    MatchInput::against(
                        &entry.opponent,
                        entry.date,
                        entry.home,
                        entry.max_substitutions,
                        &entry.notes,
                        entry.roster.clone(),
                    ),
                )
                .await?;
            match_writer
                .save_source(&created, entry.source, entry.video_clock)
                .await?
        }
    };
    for set_number in 1..=5u8 {
        match plan.entry.sets.iter().find(|s| s.set_number == set_number) {
            Some(set) => set_writer.update(&m, set).await?,
            None => set_writer.remove(&m, set_number).await?,
        }
    }
    finished.finished(saved_team.id, m.id.clone()).await;
    Ok(MatchEntry {
        sets: plan.entry.sets.clone(),
        ..m
    })
}

impl TeamIssue {
    pub fn describe(&self, labels: &Labels) -> String {
        match self {
            TeamIssue::NoYear => labels.dvw_no_year.to_string(),
            TeamIssue::Incomplete(e) => e.message(labels),
            TeamIssue::PlayerConflict { number, file, team } => labels
                .dvw_player_conflict
                .replace("{number}", &number.to_string())
                .replace("{file}", file)
                .replace("{team}", team),
        }
    }
}

fn score((us, them): (u8, u8)) -> String {
    format!("{us}-{them}")
}

impl MatchIssue {
    pub fn describe(&self, labels: &Labels) -> String {
        match self {
            MatchIssue::NoSets => labels.dvw_no_sets.to_string(),
            MatchIssue::Unfinished => labels.dvw_unfinished.to_string(),
            MatchIssue::Replay(issue) => issue.describe(labels),
            MatchIssue::SetScore {
                set_number,
                file,
                ours,
            } => labels
                .dvw_set_score
                .replace("{set}", &set_number.to_string())
                .replace("{file}", &score(*file))
                .replace("{ours}", &score(*ours)),
            MatchIssue::Set(e) => {
                let template = match e.line {
                    Some(_) => labels.dvw_set_issue_at,
                    None => labels.dvw_set_issue,
                };
                template
                    .replace("{set}", &e.set_number.to_string())
                    .replace("{line}", &e.line.map(|l| l.to_string()).unwrap_or_default())
                    .replace("{reason}", &reason(&e.issue, labels))
            }
        }
    }
}

// why a set doesn't translate, in the language of the app; the technical
// detail, in English, only for an event refused for a reason of ours
fn reason(issue: &Issue, labels: &Labels) -> String {
    match issue {
        Issue::NoSet => labels.dvw_no_set.to_string(),
        Issue::NoLineup => labels.dvw_no_lineup.to_string(),
        Issue::NoServe => labels.dvw_no_serve.to_string(),
        Issue::NoSetter => labels.dvw_no_setter.to_string(),
        Issue::NoLibero => labels.dvw_no_libero.to_string(),
        Issue::UnknownPlayer(number) => labels
            .dvw_unknown_player
            .replace("{number}", &number.to_string()),
        Issue::UnknownCode(code) => labels.dvw_unknown_code.replace("{code}", code),
        Issue::Ending { skill, evaluation } => labels
            .dvw_ending
            .replace("{code}", &format!("{}{evaluation}", skill.code())),
        Issue::Untranslatable(what) => untranslatable(what, labels),
        Issue::Refused { event, refusal } => labels
            .dvw_refused
            .replace("{event}", event.friendly_name(labels))
            .replace("{reason}", &refused(refusal, labels)),
        Issue::Event(detail) => labels.dvw_event.replace("{detail}", detail),
        Issue::Score { file, ours } => labels
            .dvw_score
            .replace("{file}", &score(*file))
            .replace("{ours}", &score(*ours)),
        Issue::Unfinished => labels.dvw_set_unfinished.to_string(),
    }
}

fn untranslatable(what: &Untranslatable, labels: &Labels) -> String {
    use Untranslatable::*;
    match what {
        SubstitutionInRally => labels.dvw_substitution_in_rally.to_string(),
        SetterChangeInRally => labels.dvw_setter_change_in_rally.to_string(),
        TheirTouchAfterTheEnd => labels.dvw_their_touch_after_end.to_string(),
        OurTouchAfterTheEnd => labels.dvw_our_touch_after_end.to_string(),
        PointWithoutCause => labels.dvw_point_without_cause.to_string(),
        LiberoTooLate => labels.dvw_libero_too_late.to_string(),
        LiberoForNobody => labels.dvw_libero_for_nobody.to_string(),
        LiberoForWhom(numbers) => labels.dvw_libero_for_whom.replace(
            "{numbers}",
            &numbers
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(labels.dvw_or),
        ),
        OurTouchWithoutPlayer => labels.dvw_our_touch_without_player.to_string(),
        TheirAttackBackWithoutOurBlock => labels.dvw_their_attack_back_without_block.to_string(),
        TheirAttackBlockedWithoutOurBlock => {
            labels.dvw_their_attack_blocked_without_block.to_string()
        }
        UnknownEvaluation(code) => labels
            .dvw_unknown_evaluation
            .replace("{code}", &code.to_string()),
    }
}

fn refused(refusal: &Refusal<u8>, labels: &Labels) -> String {
    use Refusal::*;
    let of = |template: &str, number: &u8| template.replace("{number}", &number.to_string());
    match refusal {
        SetOver => labels.dvw_refused_set_over.to_string(),
        NotNow { .. } => labels.dvw_refused_not_now.to_string(),
        NotOnCourt(n) => of(labels.dvw_refused_not_on_court, n),
        Libero(n) => of(labels.dvw_refused_libero, n),
        BackRowBlock(n) => of(labels.dvw_refused_back_row_block, n),
        NotServing(n) => of(labels.dvw_refused_not_serving, n),
        CantComeIn(n) => of(labels.dvw_refused_cant_come_in, n),
        AlreadyReplaced(n) => of(labels.dvw_refused_already_replaced, n),
        AlreadyReplacement(n) => of(labels.dvw_refused_already_replacement, n),
        LiberoReplaced => labels.dvw_refused_libero_replaced.to_string(),
        MaxSubstitutions => labels.dvw_refused_max_substitutions.to_string(),
        OnlyReplacedBy { replaced, by } => {
            of(labels.dvw_refused_only_replaced_by, replaced).replace("{by}", &by.to_string())
        }
        // the events of the translation, not of the file
        MissingEvaluation
        | EvaluationNotAllowed(_)
        | UnexpectedEvaluation
        | TechniqueNotAllowed(_)
        | MissingPlayer
        | UnexpectedPlayer
        | MissingReplacement
        | UnexpectedReplacement => labels.dvw_refused_invalid.to_string(),
    }
}
