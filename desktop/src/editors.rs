//! The forms of a team, of a player, of a match, new or saved, and of a
//! competition (only edited: created in the form of a match): the
//! fields of `form`, saved through the core, which checks them. An error
//! takes the keys to its field.

use {
    crate::{
        countries,
        form::{self, Field, Form, Mask, Outcome, Suggestion},
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL, SIZE_TITLE},
        ui,
    },
    chrono::{DateTime, FixedOffset, Local, NaiveDateTime, TimeZone},
    iced::{
        Alignment, Element, Length, Padding,
        widget::{
            Column, Id, Row, button, column, container, responsive, row, scrollable, space, text,
            text::Wrapping,
        },
    },
    scout4all_core::{
        competition_form::CompetitionForm, match_form::MatchForm, player_form::PlayerForm,
        team_form::TeamForm,
    },
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{
        ClassificationChoice, CompetitionChoice, CompetitionField, ExistingPlayer, FormatChoice,
        GenderChoice, MatchField, MatchRoster, MobileError, OpponentChoice, PlayerField,
        RoleChoice, RosterPlayer, Scout4All, SeasonPlayer, StageChoice, TeamField, TeamListing,
    },
};

/// What a key or a click did to the form.
pub enum Edited {
    Stay,
    Cancel,
    /// saved: the id of the team, of the player or of the match
    Saved(String),
}

const DATE_FORMAT: &str = "%Y-%m-%d";
const TIME_FORMAT: &str = "%H:%M";

/// The players of the score sheet in a row: as many as fit, at least
/// `ROSTER_MIN_COLUMNS`, at most `ROSTER_COLUMNS`.
const ROSTER_COLUMNS: usize = 4;
const ROSTER_MIN_COLUMNS: usize = 2;

/// The narrowest a tile of the score sheet is, a column fewer below.
const ROSTER_TILE_WIDTH: f32 = 240.0;

/// The columns of the score sheet in `width`.
fn roster_columns(width: f32) -> usize {
    let fit = ((width + 10.0) / (ROSTER_TILE_WIDTH + 10.0)).floor();
    // a finite, small number: as usize it's what fits
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let fit = fit.max(0.0) as usize;
    fit.clamp(ROSTER_MIN_COLUMNS, ROSTER_COLUMNS)
}

/// The first option of a choice that can be left empty.
const EMPTY: &str = "–";

pub struct TeamEditor {
    id: Option<String>,
    classifications: Vec<ClassificationChoice>,
    genders: Vec<GenderChoice>,
}

// the fields of the team
const TEAM_NAME: usize = 0;
const TEAM_YEAR: usize = 1;
const TEAM_CLASSIFICATION: usize = 2;
const TEAM_GENDER: usize = 3;
const TEAM_COUNTRY: usize = 4;

/// A competition (D12), only edited.
pub struct CompetitionEditor {
    id: uuid::Uuid,
    formats: Vec<FormatChoice>,
    genders: Vec<GenderChoice>,
}

// the fields of the competition
const COMPETITION_NAME: usize = 0;
const COMPETITION_SEASON: usize = 1;
const COMPETITION_FORMAT: usize = 2;
const COMPETITION_GENDER: usize = 3;
const COMPETITION_COUNTRY: usize = 4;
const COMPETITION_FRIENDLY: usize = 5;

pub struct PlayerEditor {
    team_id: String,
    id: Option<String>,
    roles: Vec<RoleChoice>,
    /// the persons of the registry offered as a new player's name is
    /// written (D5)
    existing: Vec<ExistingPlayer>,
    /// the ones of `existing` the name offers now, in their order
    offered: Vec<usize>,
    /// the person chosen, while the name is still theirs
    person: Option<ExistingPlayer>,
}

// the fields of the player
const PLAYER_NAME: usize = 0;
const PLAYER_NUMBER: usize = 1;
const PLAYER_ROLE: usize = 2;
const PLAYER_BIRTH: usize = 3;
const PLAYER_HEIGHT: usize = 4;
const PLAYER_NOTES: usize = 5;

pub struct MatchEditor {
    /// team A: chosen among `teams` for a new match, then fixed
    team_id: String,
    teams: Vec<(String, String)>,
    id: Option<String>,
    /// the form as the core keeps it: the roster, the offset of the date,
    /// team B and the competition chosen
    data: MatchForm,
    roster: Vec<RosterPlayer>,
    /// the other teams and the names met, offered as team B is written
    opponents: Vec<OpponentChoice>,
    competitions: Vec<CompetitionChoice>,
    formats: Vec<FormatChoice>,
    stages: Vec<StageChoice>,
    step: MatchStep,
    /// the number and role of a player of the score sheet being corrected
    correcting: Option<Correction>,
    /// the columns of the score sheet as last drawn, for the arrows
    columns: std::cell::Cell<usize>,
}

/// The number and the role a player has in the match, corrected in a
/// small form over the score sheet (D9).
struct Correction {
    player: uuid::Uuid,
    roles: Vec<RoleChoice>,
    form: Form,
}

// the fields of a correction
const CORRECTION_NUMBER: usize = 0;
const CORRECTION_ROLE: usize = 1;

/// The new season of a team (D15): the new team, as the team form writes
/// it, then its players among the ones of the team, on tiles as the score
/// sheet of a match.
pub struct SeasonEditor {
    /// the team of the season before
    team_id: String,
    classifications: Vec<ClassificationChoice>,
    genders: Vec<GenderChoice>,
    players: Vec<SeasonPlayer>,
    /// the players as tiles
    tiles: Vec<RosterPlayer>,
    /// on the players
    on_players: bool,
    correcting: Option<Correction>,
    columns: std::cell::Cell<usize>,
}

/// The fields of the new season: the ones of the team, then a check for
/// each player.
const SEASON_PLAYERS: usize = TEAM_COUNTRY + 1;

/// A player of the new season on a tile.
fn season_tile(p: &SeasonPlayer) -> RosterPlayer {
    RosterPlayer {
        id: p.id,
        name: p.name.clone(),
        number: p.number,
        role: p.role_name.clone(),
        chosen: p.chosen,
        played: false,
        role_value: p.role,
    }
}

/// The steps of the form of a match: the teams and when, the competition
/// and the rules, the players of the score sheet (as on Android).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatchStep {
    Match,
    Competition,
    Roster,
}

impl MatchStep {
    /// The fields of the step.
    fn fields(self, all: usize) -> std::ops::Range<usize> {
        match self {
            MatchStep::Match => MATCH_TEAM..MATCH_COMPETITION,
            MatchStep::Competition => MATCH_COMPETITION..MATCH_ROSTER,
            MatchStep::Roster => MATCH_ROSTER..all.max(MATCH_ROSTER),
        }
    }

    /// The step of `field`.
    fn of(field: usize) -> MatchStep {
        match field {
            f if f < MATCH_COMPETITION => MatchStep::Match,
            f if f < MATCH_ROSTER => MatchStep::Competition,
            _ => MatchStep::Roster,
        }
    }

    fn next(self) -> Option<MatchStep> {
        match self {
            MatchStep::Match => Some(MatchStep::Competition),
            MatchStep::Competition => Some(MatchStep::Roster),
            MatchStep::Roster => None,
        }
    }

    fn previous(self) -> Option<MatchStep> {
        match self {
            MatchStep::Match => None,
            MatchStep::Competition => Some(MatchStep::Match),
            MatchStep::Roster => Some(MatchStep::Competition),
        }
    }
}

// the fields of the match in their steps, then a check for each player of
// the roster
const MATCH_TEAM: usize = 0;
const MATCH_OPPONENT: usize = 1;
const MATCH_CREATE_OPPONENT: usize = 2;
const MATCH_DATE: usize = 3;
const MATCH_TIME: usize = 4;
const MATCH_COURT: usize = 5;
const MATCH_COMPETITION: usize = 6;
const MATCH_FORMAT: usize = 7;
const MATCH_FRIENDLY: usize = 8;
const MATCH_STAGE: usize = 9;
const MATCH_ROUND: usize = 10;
const MATCH_SUBSTITUTIONS: usize = 11;
const MATCH_NOTES: usize = 12;
const MATCH_ROSTER: usize = 13;

// the options of the court
const COURT_HOME: usize = 0;
const COURT_AWAY: usize = 1;
const COURT_NEUTRAL: usize = 2;

/// The option of the court of the form.
fn court_of(form: &MatchForm) -> usize {
    match (form.neutral, form.home) {
        (true, _) => COURT_NEUTRAL,
        (false, true) => COURT_HOME,
        (false, false) => COURT_AWAY,
    }
}

/// The name of a team or a competition as written, to compare.
fn same_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// The team written: the one linked if it's still its name, else the
/// first with that name (the most recent year, as offered).
fn linked_team(
    choices: &[OpponentChoice],
    written: &str,
    linked: Option<uuid::Uuid>,
) -> Option<uuid::Uuid> {
    let named = |id: uuid::Uuid| {
        choices
            .iter()
            .any(|c| c.team == Some(id) && same_name(&c.name, written))
    };
    linked.filter(|id| named(*id)).or_else(|| {
        choices
            .iter()
            .find(|c| c.team.is_some() && same_name(&c.name, written))
            .and_then(|c| c.team)
    })
}

/// The competition written: the one linked if it's still its name, else
/// the first with that name (the most recent season, as offered).
fn linked_competition(
    choices: &[CompetitionChoice],
    written: &str,
    linked: Option<uuid::Uuid>,
) -> Option<uuid::Uuid> {
    let named = |id: uuid::Uuid| {
        choices
            .iter()
            .any(|c| c.id == id && same_name(&c.name, written))
    };
    linked.filter(|id| named(*id)).or_else(|| {
        choices
            .iter()
            .find(|c| same_name(&c.name, written))
            .map(|c| c.id)
    })
}

pub enum Kind {
    Team(TeamEditor),
    Competition(CompetitionEditor),
    Player(Box<PlayerEditor>),
    Match(Box<MatchEditor>),
    Season(Box<SeasonEditor>),
}

/// The tiles of a score sheet (of a match, or of a new season) being
/// shown: the field of the first, the players, the columns drawn.
struct Sheet<'a> {
    first: usize,
    tiles: &'a [RosterPlayer],
    columns: &'a std::cell::Cell<usize>,
}

pub struct Editor {
    pub form: Form,
    kind: Kind,
    /// another player of the team with the name of the one being saved,
    /// as the core tells: saved only once confirmed
    namesake: Option<String>,
}

/// The option of `value` among `values`, after the empty one.
fn option_of<T: PartialEq>(values: impl Iterator<Item = T>, value: Option<T>) -> usize {
    value
        .and_then(|v| values.into_iter().position(|x| x == v))
        .map_or(0, |i| i + 1)
}

fn with_empty(names: impl Iterator<Item = String>) -> Vec<String> {
    std::iter::once(EMPTY.to_string()).chain(names).collect()
}

fn date_text(date: Option<DateTime<FixedOffset>>, format: &str) -> String {
    date.map(|d| d.format(format).to_string())
        .unwrap_or_default()
}

/// The date and the time written, at the offset of the one they replace
/// (where the match is played), else at the one of here; none if they
/// can't be read.
fn parse_date(
    day: &str,
    time: &str,
    was: Option<DateTime<FixedOffset>>,
) -> Option<DateTime<FixedOffset>> {
    let naive = NaiveDateTime::parse_from_str(
        &format!("{} {}", day.trim(), time.trim()),
        &format!("{DATE_FORMAT} {TIME_FORMAT}"),
    )
    .ok()?;
    match was {
        Some(was) => was.offset().from_local_datetime(&naive).single(),
        None => Local
            .from_local_datetime(&naive)
            .single()
            .map(|d| d.fixed_offset()),
    }
}

/// The fields of a team, from `start`, with the classifications and the
/// genders they offer.
fn team_fields(
    core: &Scout4All,
    s: &Strings,
    start: &TeamForm,
) -> (Vec<Field>, Vec<ClassificationChoice>, Vec<GenderChoice>) {
    let classifications = core.classifications();
    let genders = core.genders();
    let country = start
        .country
        .map(|c| core.country_name(c))
        .unwrap_or_default();
    let fields = vec![
        Field::text(s.name, start.name.clone()),
        Field::text(s.year, start.year.clone()),
        Field::choice(
            s.classification,
            with_empty(classifications.iter().map(|c| c.name.clone())),
            option_of(
                classifications.iter().map(|c| c.value),
                start.classification,
            ),
        ),
        Field::choice(
            s.gender,
            with_empty(genders.iter().map(|g| g.name.clone())),
            option_of(genders.iter().map(|g| g.value), start.gender),
        ),
        Field::dropdown(s.country, country),
    ];
    (fields, classifications, genders)
}

/// The team as its fields have it.
fn team_form_of(
    core: &Scout4All,
    s: &Strings,
    f: &Form,
    classifications: &[ClassificationChoice],
    genders: &[GenderChoice],
) -> Result<TeamForm, MobileError> {
    let country =
        countries::chosen(core, &f.text(TEAM_COUNTRY)).map_err(|()| MobileError::InvalidTeam {
            field: TeamField::Country,
            reason: s.no_country_found.to_string(),
        })?;
    Ok(TeamForm {
        name: f.text(TEAM_NAME),
        year: f.text(TEAM_YEAR),
        classification: f
            .index(TEAM_CLASSIFICATION)
            .checked_sub(1)
            .and_then(|i| classifications.get(i))
            .map(|c| c.value),
        gender: f
            .index(TEAM_GENDER)
            .checked_sub(1)
            .and_then(|i| genders.get(i))
            .map(|g| g.value),
        country,
    })
}

impl Editor {
    pub fn team(core: &Scout4All, s: &Strings, id: Option<String>, start: TeamForm) -> Editor {
        let (fields, classifications, genders) = team_fields(core, s, &start);
        let title = match id {
            Some(_) => s.edit_team,
            None => s.new_team,
        };
        Editor {
            form: Form::new(title, fields),
            namesake: None,
            kind: Kind::Team(TeamEditor {
                id,
                classifications,
                genders,
            }),
        }
    }

    /// The competition `id` to edit, as saved.
    pub fn competition(
        core: &Scout4All,
        s: &Strings,
        id: uuid::Uuid,
    ) -> Result<Editor, MobileError> {
        let start = core.competition_form(id)?;
        let formats = core.competition_formats();
        let genders = core.genders();
        let country = start
            .country
            .map(|c| core.country_name(c))
            .unwrap_or_default();
        let fields = vec![
            Field::text(s.name, start.name.clone()),
            Field::text(s.season, start.season.clone()).hint(s.season_placeholder),
            Field::choice(
                s.competition_format,
                formats.iter().map(|f| f.name.clone()).collect(),
                formats
                    .iter()
                    .position(|f| f.value == start.format)
                    .unwrap_or(0),
            ),
            Field::choice(
                s.gender,
                with_empty(genders.iter().map(|g| g.name.clone())),
                option_of(genders.iter().map(|g| g.value), start.gender),
            ),
            Field::dropdown(s.country, country),
            Field::check(s.friendly_default, start.friendly, false),
        ];
        Ok(Editor {
            form: Form::new(s.edit_competition, fields),
            namesake: None,
            kind: Kind::Competition(CompetitionEditor {
                id,
                formats,
                genders,
            }),
        })
    }

    pub fn player(
        s: &Strings,
        core: &Scout4All,
        team_id: String,
        id: Option<String>,
        start: PlayerForm,
    ) -> Editor {
        let roles = core.roles(team_id.clone());
        // a new player may be a person of the registry; one saved may play
        // in other teams too, with the same details
        let existing = match &id {
            Some(_) => vec![],
            None => core.existing_players(team_id.clone()).unwrap_or_default(),
        };
        let others = id
            .as_ref()
            .and_then(|id| core.other_teams(team_id.clone(), id.clone()).ok())
            .unwrap_or_default();
        let name = match (existing.is_empty(), others.is_empty()) {
            (false, _) => Field::search(s.name, start.name.clone()).hint(s.name_or_existing),
            (true, false) => Field::text(s.name, start.name.clone())
                .hint(&ui::fill(s.plays_also_in, &[others.join(", ")])),
            (true, true) => Field::text(s.name, start.name.clone()),
        };
        let fields = vec![
            name,
            Field::text(s.number, start.number.clone()),
            Field::choice(
                s.role,
                with_empty(roles.iter().map(|r| r.name.clone())),
                option_of(roles.iter().map(|r| r.value), start.role),
            ),
            Field::masked(
                s.birth_date,
                &start.birth_date,
                Mask::Date,
                s.date_placeholder,
            ),
            Field::text(s.height, start.height_cm.clone()),
            Field::text(s.notes, start.notes.clone()),
        ];
        let title = match id {
            Some(_) => s.edit_player,
            None => s.new_player,
        };
        Editor {
            form: Form::new(title, fields),
            namesake: None,
            kind: Kind::Player(Box::new(PlayerEditor {
                team_id,
                id,
                roles,
                existing,
                offered: vec![],
                person: None,
            })),
        }
    }

    /// The new season of the team `team_id` (D15): the team the year
    /// after, then its players.
    pub fn season(core: &Scout4All, s: &Strings, team_id: String) -> Result<Editor, MobileError> {
        let season = core.new_season(team_id.clone())?;
        let (fields, classifications, genders) = team_fields(core, s, &season.form);
        let mut form = Form::new(s.new_season, fields);
        form.actions = Some((
            (theme::icons::CLOSE, s.cancel.to_string()),
            (theme::icons::FORWARD_ARROW, s.go_on.to_string()),
        ));
        let mut editor = Editor {
            form,
            namesake: None,
            kind: Kind::Season(Box::new(SeasonEditor {
                team_id,
                classifications,
                genders,
                players: vec![],
                tiles: vec![],
                on_players: false,
                correcting: None,
                columns: std::cell::Cell::new(ROSTER_COLUMNS),
            })),
        };
        editor.set_season_players(season.players);
        editor.show_season(false);
        Ok(editor)
    }

    /// The players of the new season, one check for each.
    fn set_season_players(&mut self, players: Vec<SeasonPlayer>) {
        self.form.fields.truncate(SEASON_PLAYERS);
        self.form.fields.extend(
            players
                .iter()
                .map(|p| Field::check(&p.name, p.chosen, false)),
        );
        if let Kind::Season(t) = &mut self.kind {
            t.tiles = players.iter().map(season_tile).collect();
            t.players = players;
        }
    }

    /// The team of the new season, or its players: the keys on the first
    /// field.
    fn show_season(&mut self, players: bool) {
        if let Kind::Season(t) = &mut self.kind {
            t.on_players = players;
            let fields = match players {
                true => SEASON_PLAYERS..self.form.fields.len().max(SEASON_PLAYERS),
                false => TEAM_NAME..SEASON_PLAYERS,
            };
            self.form.focused = fields.start;
            self.form.shown = Some(fields);
        }
    }

    /// The persons of the registry the name written offers, with the teams
    /// they play in; the one chosen let go once the name isn't theirs.
    fn offer_persons(&mut self, s: &Strings) {
        let Kind::Player(p) = &mut self.kind else {
            return;
        };
        let written = self.form.text(PLAYER_NAME);
        if p.person
            .as_ref()
            .is_some_and(|person| !same_name(&person.name, &written))
        {
            p.person = None;
            if let Some(field) = self.form.fields.get_mut(PLAYER_NAME) {
                field.hint = Some(s.name_or_existing.to_string());
            }
        }
        let labels: Vec<String> = p.existing.iter().map(|e| e.label.clone()).collect();
        let offered = scout4all_mobile::offered_names(labels, written);
        // two with the same label: each once, in its order
        let mut taken: Vec<usize> = vec![];
        for label in &offered {
            if let Some(i) = p
                .existing
                .iter()
                .enumerate()
                .position(|(i, e)| e.label == *label && !taken.contains(&i))
            {
                taken.push(i);
            }
        }
        let suggestions = taken
            .iter()
            .filter_map(|i| p.existing.get(*i))
            .map(|e| Suggestion {
                label: e.label.clone(),
                value: e.name.clone(),
            })
            .collect();
        p.offered = taken;
        self.form.suggest(PLAYER_NAME, suggestions);
    }

    /// The person of the registry offered `n`th taken: the form gets their
    /// details, and the number and the role of their most recent team.
    fn take_person(&mut self, s: &Strings, n: usize) {
        let Kind::Player(p) = &mut self.kind else {
            return;
        };
        let Some(person) = p.offered.get(n).and_then(|i| p.existing.get(*i)).cloned() else {
            return;
        };
        let start = &person.form;
        let role = option_of(p.roles.iter().map(|r| r.value), start.role);
        let teams = person
            .label
            .strip_prefix(&person.name)
            .map(|rest| rest.trim().trim_start_matches('(').trim_end_matches(')'))
            .unwrap_or_default()
            .to_string();
        let hint = match teams.is_empty() {
            true => s.in_registry.to_string(),
            false => format!(
                "{} · {}",
                s.in_registry,
                ui::fill(s.plays_also_in, &[teams])
            ),
        };
        let values = [
            (PLAYER_NUMBER, Field::text("", start.number.clone()).value),
            (
                PLAYER_ROLE,
                form::Value::Choice {
                    options: with_empty(p.roles.iter().map(|r| r.name.clone())),
                    index: role,
                },
            ),
            (
                PLAYER_BIRTH,
                Field::masked("", &start.birth_date, Mask::Date, "").value,
            ),
            (
                PLAYER_HEIGHT,
                Field::text("", start.height_cm.clone()).value,
            ),
            (PLAYER_NOTES, Field::text("", start.notes.clone()).value),
        ];
        for (field, value) in values {
            if let Some(f) = self.form.fields.get_mut(field) {
                f.value = value;
            }
        }
        if let Some(f) = self.form.fields.get_mut(PLAYER_NAME) {
            f.hint = Some(hint);
        }
        p.person = Some(person);
    }

    /// A new match (no `id`) of team A `team_id`, on the date of now, or
    /// a saved one.
    pub fn a_match(
        s: &Strings,
        core: &Scout4All,
        team_id: String,
        id: Option<String>,
    ) -> Result<Editor, MobileError> {
        let data = match &id {
            Some(id) => core.match_form(team_id.clone(), id.clone())?,
            None => MatchForm::new(Local::now().fixed_offset()),
        };
        // a saved match keeps its team A
        let teams: Vec<(String, String)> = core
            .teams()?
            .into_iter()
            .filter_map(|t| match t {
                TeamListing::Team { id, name, year, .. } => Some((id, format!("{name} ({year})"))),
                TeamListing::Unreadable { .. } => None,
            })
            .filter(|(team, _)| id.is_none() || *team == team_id)
            .collect();
        let team_index = teams.iter().position(|(t, _)| *t == team_id).unwrap_or(0);
        let competitions = core.competitions()?;
        let formats = core.competition_formats();
        let stages = core.stages();
        let fields = vec![
            Field::choice(
                s.team_a,
                teams.iter().map(|(_, name)| name.clone()).collect(),
                team_index,
            ),
            Field::search(s.opponent, data.opponent.clone()),
            Field::check(s.create_opponent, data.create_opponent, false),
            Field::masked(
                s.date,
                &date_text(data.date, DATE_FORMAT),
                Mask::Date,
                s.date_placeholder,
            ),
            Field::masked(
                s.time,
                &date_text(data.date, TIME_FORMAT),
                Mask::Time,
                s.time_placeholder,
            ),
            Field::segments(
                "",
                vec![
                    s.home_game.to_string(),
                    s.away_game.to_string(),
                    s.neutral_game.to_string(),
                ],
                court_of(&data),
            )
            .icons(vec![
                theme::icons::HOME,
                theme::icons::AWAY,
                theme::icons::VOLLEYBALL,
            ]),
            Field::search(s.competition, data.competition.clone()),
            Field::choice(
                s.competition_format,
                formats.iter().map(|f| f.name.clone()).collect(),
                data.competition_format
                    .and_then(|v| formats.iter().position(|f| f.value == v))
                    .unwrap_or(0),
            ),
            Field::check(s.friendly, data.friendly, false),
            Field::choice(
                s.stage,
                with_empty(stages.iter().map(|st| st.name.clone())),
                option_of(stages.iter().map(|st| st.value), data.stage),
            ),
            Field::text(s.round, data.round.clone()).placeholder(s.round_placeholder),
            Field::text(s.max_substitutions, data.max_substitutions.clone()),
            Field::text(s.notes, data.notes.clone()),
        ];
        let title = match id {
            Some(_) => s.edit_match,
            None => s.new_match,
        };
        let mut form = Form::new(title, fields);
        form.sections = vec![
            (MATCH_TEAM, s.match_section.to_string()),
            (MATCH_COMPETITION, s.competition_section.to_string()),
        ];
        form.actions = Some((
            (theme::icons::CLOSE, s.cancel.to_string()),
            (theme::icons::FORWARD_ARROW, s.go_on.to_string()),
        ));
        let opponents = core.opponent_choices(team_id.clone()).unwrap_or_default();
        let mut editor = Editor {
            form,
            namesake: None,
            kind: Kind::Match(Box::new(MatchEditor {
                team_id,
                teams,
                id,
                data,
                roster: vec![],
                opponents,
                competitions,
                formats,
                stages,
                step: MatchStep::Match,
                correcting: None,
                columns: std::cell::Cell::new(ROSTER_COLUMNS),
            })),
        };
        editor.show(MatchStep::Match);
        editor.linked();
        editor.toggle(core, None)?;
        Ok(editor)
    }

    /// The fields of `step`, the keys on its first one.
    fn show(&mut self, step: MatchStep) {
        if let Kind::Match(m) = &mut self.kind {
            m.step = step;
            let fields = step.fields(self.form.fields.len());
            self.form.focused = fields.start;
            self.form.shown = Some(fields);
        }
    }

    /// Team B and the competition linked to what's written, and the fields
    /// that apply shown: "create as a team" for a name that isn't a team,
    /// the format for a new competition. A competition just chosen gives
    /// its friendly.
    fn linked(&mut self) {
        let Kind::Match(m) = &mut self.kind else {
            return;
        };
        let f = &mut self.form;
        let opponent = f.text(MATCH_OPPONENT);
        // A isn't among the teams offered
        m.data.opponent_team = linked_team(&m.opponents, &opponent, m.data.opponent_team);
        let competition = f.text(MATCH_COMPETITION);
        let was = m.data.competition_id;
        m.data.competition_id = linked_competition(&m.competitions, &competition, was);
        let chosen = m
            .competitions
            .iter()
            .find(|c| Some(c.id) == m.data.competition_id);
        let friendly = f.fields.get_mut(MATCH_FRIENDLY);
        if let (Some(c), true, Some(field)) = (chosen, m.data.competition_id != was, friendly) {
            field.value = form::Value::Check {
                on: c.friendly,
                fixed: false,
            };
        }
        f.hidden = [
            (
                MATCH_CREATE_OPPONENT,
                opponent.trim().is_empty() || m.data.opponent_team.is_some(),
            ),
            (
                MATCH_FORMAT,
                competition.trim().is_empty() || m.data.competition_id.is_some(),
            ),
        ]
        .into_iter()
        .filter_map(|(field, hidden)| hidden.then_some(field))
        .collect();
    }

    /// The match form as written, the roster and what's linked as the
    /// core keeps them.
    fn match_data(&self, m: &MatchEditor) -> MatchForm {
        let f = &self.form;
        let court = f.index(MATCH_COURT);
        // "create as a team" left on for a team: the team wins
        let checked = |field: usize| {
            matches!(
                f.fields.get(field).map(|x| &x.value),
                Some(form::Value::Check { on: true, .. })
            )
        };
        MatchForm {
            opponent: f.text(MATCH_OPPONENT),
            date: parse_date(&f.text(MATCH_DATE), &f.text(MATCH_TIME), m.data.date),
            home: court == COURT_HOME,
            max_substitutions: f.text(MATCH_SUBSTITUTIONS),
            notes: f.text(MATCH_NOTES),
            roster: m.data.roster.clone(),
            opponent_team: m.data.opponent_team,
            create_opponent: checked(MATCH_CREATE_OPPONENT),
            neutral: court == COURT_NEUTRAL,
            friendly: checked(MATCH_FRIENDLY),
            competition: f.text(MATCH_COMPETITION),
            competition_id: m.data.competition_id,
            competition_format: m
                .formats
                .get(f.index(MATCH_FORMAT))
                .map(|format| format.value),
            stage: f
                .index(MATCH_STAGE)
                .checked_sub(1)
                .and_then(|i| m.stages.get(i))
                .map(|st| st.value),
            round: f.text(MATCH_ROUND),
            numbers: m.data.numbers.clone(),
        }
    }

    /// The roster with `toggled` put on it or taken off, as the core
    /// allows: one check for each player.
    fn toggle(&mut self, core: &Scout4All, toggled: Option<uuid::Uuid>) -> Result<(), MobileError> {
        let Kind::Match(m) = &self.kind else {
            return Ok(());
        };
        let data = self.match_data(m);
        let roster = core.match_roster(m.team_id.clone(), m.id.clone(), data, toggled)?;
        self.set_roster(roster);
        Ok(())
    }

    /// The roster as the core gives it back: one check for each player.
    fn set_roster(&mut self, roster: MatchRoster) {
        self.form.fields.truncate(MATCH_ROSTER);
        self.form.fields.extend(
            roster
                .players
                .iter()
                .map(|p| Field::check(&p.name, p.chosen, p.played)),
        );
        if let Kind::Match(m) = &mut self.kind {
            m.data = roster.form;
            m.roster = roster.players;
        }
    }

    /// The small form of the number and the role of the player of the
    /// score sheet at `field`.
    fn open_correction(&mut self, core: &Scout4All, s: &Strings, field: usize) {
        let Some(p) = self
            .sheet()
            .and_then(|sheet| {
                field
                    .checked_sub(sheet.first)
                    .and_then(|n| sheet.tiles.get(n))
            })
            .cloned()
        else {
            return;
        };
        let team = match &self.kind {
            Kind::Match(m) => m.team_id.clone(),
            Kind::Season(t) => t.team_id.clone(),
            Kind::Team(_) | Kind::Competition(_) | Kind::Player(_) => return,
        };
        let roles = core.roles(team);
        let fields = vec![
            Field::text(s.number, p.number.to_string()),
            Field::choice(
                s.role,
                with_empty(roles.iter().map(|r| r.name.clone())),
                option_of(roles.iter().map(|r| r.value), p.role_value),
            ),
        ];
        let mut form = Form::new(&format!("{} {}", p.number, p.name), fields);
        form.actions = Some((
            (theme::icons::CLOSE, s.cancel.to_string()),
            (theme::icons::CHECK, s.ok.to_string()),
        ));
        self.form.focused = field;
        if let Some(slot) = self.correcting_slot() {
            *slot = Some(Correction {
                player: p.id,
                roles,
                form,
            });
        }
    }

    /// Where the correction of a score sheet goes, for the kinds that have
    /// one.
    fn correcting_slot(&mut self) -> Option<&mut Option<Correction>> {
        match &mut self.kind {
            Kind::Match(m) => Some(&mut m.correcting),
            Kind::Season(t) => Some(&mut t.correcting),
            Kind::Team(_) | Kind::Competition(_) | Kind::Player(_) => None,
        }
    }

    /// The correction open, if any.
    fn correcting(&self) -> Option<&Correction> {
        match &self.kind {
            Kind::Match(m) => m.correcting.as_ref(),
            Kind::Season(t) => t.correcting.as_ref(),
            Kind::Team(_) | Kind::Competition(_) | Kind::Player(_) => None,
        }
    }

    /// The tiles shown, on the step of the score sheet or of the players
    /// of a new season.
    fn sheet(&self) -> Option<Sheet<'_>> {
        match &self.kind {
            Kind::Match(m) if m.step == MatchStep::Roster => Some(Sheet {
                first: MATCH_ROSTER,
                tiles: &m.roster,
                columns: &m.columns,
            }),
            Kind::Season(t) if t.on_players => Some(Sheet {
                first: SEASON_PLAYERS,
                tiles: &t.tiles,
                columns: &t.columns,
            }),
            _ => None,
        }
    }

    /// The correction saved in the form of the match (or among the
    /// players of the new season), the keys on the player (wherever its
    /// number puts it now); or its error shown.
    fn corrected(&mut self, core: &Scout4All) {
        enum Corrected {
            Roster(Box<MatchRoster>),
            Season(Vec<SeasonPlayer>),
        }
        let Some(c) = self.correcting() else {
            return;
        };
        let role = c
            .form
            .index(CORRECTION_ROLE)
            .checked_sub(1)
            .and_then(|i| c.roles.get(i))
            .map(|r| r.value);
        let player = c.player;
        let number = c.form.text(CORRECTION_NUMBER);
        let corrected = match &self.kind {
            Kind::Match(m) => core
                .correct_roster_number(
                    m.team_id.clone(),
                    m.id.clone(),
                    self.match_data(m),
                    player,
                    number,
                    role,
                )
                .map(|r| Corrected::Roster(Box::new(r))),
            Kind::Season(t) => core
                .correct_season_player(t.team_id.clone(), t.players.clone(), player, number, role)
                .map(Corrected::Season),
            Kind::Team(_) | Kind::Competition(_) | Kind::Player(_) => return,
        };
        match corrected {
            Ok(Corrected::Roster(roster)) => self.set_roster(*roster),
            Ok(Corrected::Season(players)) => self.set_season_players(players),
            Err(e) => {
                if let Some(Some(c)) = self.correcting_slot() {
                    c.form.error = Some(e.to_string());
                }
                return;
            }
        }
        if let Some(slot) = self.correcting_slot() {
            *slot = None;
        }
        let at = self.sheet().and_then(|sheet| {
            sheet
                .tiles
                .iter()
                .position(|p| p.id == player)
                .map(|n| sheet.first + n)
        });
        if let Some(field) = at {
            self.form.focused = field;
        }
    }

    /// What the small form of a correction did: confirmed, closed.
    fn correction_outcome(&mut self, core: &Scout4All, outcome: Outcome) -> Edited {
        match outcome {
            Outcome::Save => self.corrected(core),
            Outcome::Cancel => {
                if let Some(slot) = self.correcting_slot() {
                    *slot = None;
                }
            }
            Outcome::Stay | Outcome::Changed(_) => {}
        }
        Edited::Stay
    }

    /// The small form of a correction, when open.
    fn correction(&mut self) -> Option<&mut Form> {
        self.correcting_slot()
            .and_then(|c| c.as_mut())
            .map(|c| &mut c.form)
    }

    fn changed(&mut self, core: &Scout4All, s: &Strings, field: usize) {
        match (&mut self.kind, field) {
            (Kind::Player(p), PLAYER_NAME) if p.id.is_none() => self.offer_persons(s),
            // a player taken to the new season, or left
            (Kind::Season(t), f) if f >= SEASON_PLAYERS => {
                let mut players = t.players.clone();
                if let Some(p) = players.get_mut(f - SEASON_PLAYERS) {
                    p.chosen = !p.chosen;
                }
                self.set_season_players(players);
            }
            (Kind::Team(_) | Kind::Season(_), TEAM_COUNTRY) => {
                let (written, all) = (
                    self.form.text(TEAM_COUNTRY),
                    self.form.browsing(TEAM_COUNTRY),
                );
                self.form
                    .suggest(TEAM_COUNTRY, countries::offered(core, &written, all));
            }
            (Kind::Competition(_), COMPETITION_COUNTRY) => {
                let (written, all) = (
                    self.form.text(COMPETITION_COUNTRY),
                    self.form.browsing(COMPETITION_COUNTRY),
                );
                self.form
                    .suggest(COMPETITION_COUNTRY, countries::offered(core, &written, all));
            }
            // another team A: its players, the teams it can meet
            (Kind::Match(m), MATCH_TEAM) => {
                let chosen = m.teams.get(self.form.index(MATCH_TEAM)).map(|(id, _)| id);
                if let Some(team) = chosen.filter(|team| **team != m.team_id) {
                    m.team_id = team.clone();
                    m.data.roster = None;
                    m.data.numbers.clear();
                    m.opponents = core.opponent_choices(team.clone()).unwrap_or_default();
                }
                self.linked();
                if let Err(e) = self.toggle(core, None) {
                    self.form.error = Some(e.to_string());
                }
            }
            (Kind::Match(m), MATCH_OPPONENT) => {
                let names = m.opponents.iter().map(|o| o.name.clone()).collect();
                let offered =
                    scout4all_mobile::offered_names(names, self.form.text(MATCH_OPPONENT))
                        .into_iter()
                        .map(|name| {
                            let label = m
                                .opponents
                                .iter()
                                .find(|o| o.name == name)
                                .and_then(|o| o.year)
                                .map_or_else(|| name.clone(), |year| format!("{name} ({year})"));
                            Suggestion { label, value: name }
                        })
                        .collect();
                self.form.suggest(MATCH_OPPONENT, offered);
                self.linked();
            }
            (Kind::Match(m), MATCH_COMPETITION) => {
                let names = m.competitions.iter().map(|c| c.name.clone()).collect();
                let offered =
                    scout4all_mobile::offered_names(names, self.form.text(MATCH_COMPETITION))
                        .into_iter()
                        .map(|name| {
                            let label = m
                                .competitions
                                .iter()
                                .find(|c| c.name == name)
                                .and_then(|c| c.season.clone())
                                .map_or_else(
                                    || name.clone(),
                                    |season| format!("{name} ({season})"),
                                );
                            Suggestion { label, value: name }
                        })
                        .collect();
                self.form.suggest(MATCH_COMPETITION, offered);
                self.linked();
            }
            (Kind::Match(m), f) if f >= MATCH_ROSTER => {
                let toggled = m.roster.get(f - MATCH_ROSTER).map(|p| p.id);
                if let Err(e) = self.toggle(core, toggled) {
                    self.form.error = Some(e.to_string());
                }
            }
            _ => {}
        }
    }

    /// Saves the form; a player with the name of another one of the team
    /// only when `namesake_confirmed`.
    fn save(
        &mut self,
        core: &Scout4All,
        s: &Strings,
        namesake_confirmed: bool,
    ) -> Result<String, MobileError> {
        let f = &self.form;
        match &self.kind {
            Kind::Team(t) => {
                let form = team_form_of(core, s, f, &t.classifications, &t.genders)?;
                core.save_team(t.id.clone(), form)
            }
            Kind::Season(t) => {
                let form = team_form_of(core, s, f, &t.classifications, &t.genders)?;
                core.save_new_season(t.team_id.clone(), form, t.players.clone())
            }
            Kind::Competition(c) => {
                let country =
                    countries::chosen(core, &f.text(COMPETITION_COUNTRY)).map_err(|()| {
                        MobileError::InvalidCompetition {
                            field: CompetitionField::Country,
                            reason: s.no_country_found.to_string(),
                        }
                    })?;
                let form = CompetitionForm {
                    name: f.text(COMPETITION_NAME),
                    season: f.text(COMPETITION_SEASON),
                    format: c.formats.get(f.index(COMPETITION_FORMAT)).map_or(
                        scout4all_core::domain::CompetitionFormat::League,
                        |format| format.value,
                    ),
                    gender: f
                        .index(COMPETITION_GENDER)
                        .checked_sub(1)
                        .and_then(|i| c.genders.get(i))
                        .map(|g| g.value),
                    country,
                    friendly: matches!(
                        f.fields.get(COMPETITION_FRIENDLY).map(|x| &x.value),
                        Some(form::Value::Check { on: true, .. })
                    ),
                };
                core.save_competition(c.id, form).map(|()| c.id.to_string())
            }
            Kind::Player(p) => {
                let form = PlayerForm {
                    name: f.text(PLAYER_NAME),
                    number: f.text(PLAYER_NUMBER),
                    role: f
                        .index(PLAYER_ROLE)
                        .checked_sub(1)
                        .and_then(|i| p.roles.get(i))
                        .map(|r| r.value),
                    birth_date: f.text(PLAYER_BIRTH),
                    height_cm: f.text(PLAYER_HEIGHT),
                    notes: f.text(PLAYER_NOTES),
                    person: p.person.as_ref().map(|person| person.id),
                };
                core.save_player(p.team_id.clone(), p.id.clone(), form, namesake_confirmed)
            }
            Kind::Match(m) => core.save_match(m.team_id.clone(), m.id.clone(), self.match_data(m)),
        }
    }

    /// The field the error is about: the keys go there.
    fn field_of(&self, e: &MobileError) -> Option<usize> {
        match e {
            MobileError::InvalidTeam { field, .. } => Some(match field {
                TeamField::Name => TEAM_NAME,
                TeamField::Year => TEAM_YEAR,
                TeamField::Classification => TEAM_CLASSIFICATION,
                TeamField::Gender => TEAM_GENDER,
                TeamField::Country => TEAM_COUNTRY,
            }),
            // a number of the new season: on its players
            MobileError::InvalidPlayer { .. } if matches!(self.kind, Kind::Season(_)) => {
                Some(SEASON_PLAYERS)
            }
            MobileError::InvalidPlayer { field, .. } => Some(match field {
                PlayerField::Name => PLAYER_NAME,
                PlayerField::Number => PLAYER_NUMBER,
                PlayerField::Role => PLAYER_ROLE,
                PlayerField::BirthDate => PLAYER_BIRTH,
                PlayerField::Height => PLAYER_HEIGHT,
            }),
            MobileError::InvalidCompetition { field, .. } => Some(match field {
                CompetitionField::Name => COMPETITION_NAME,
                CompetitionField::Country => COMPETITION_COUNTRY,
            }),
            MobileError::InvalidMatch { field, .. } => Some(match field {
                MatchField::Opponent => MATCH_OPPONENT,
                MatchField::Date => MATCH_DATE,
                MatchField::MaxSubstitutions => MATCH_SUBSTITUTIONS,
                MatchField::Roster => MATCH_ROSTER,
            }),
            _ => None,
        }
    }

    fn outcome(&mut self, core: &Scout4All, s: &Strings, outcome: Outcome) -> Edited {
        match outcome {
            Outcome::Stay => Edited::Stay,
            // the score sheet: back to the details
            Outcome::Cancel if self.step(true) => Edited::Stay,
            Outcome::Cancel => Edited::Cancel,
            Outcome::Changed(field) => {
                self.changed(core, s, field);
                Edited::Stay
            }
            // the details of a match, then its score sheet
            Outcome::Save if self.step(false) => {
                self.form.error = None;
                Edited::Stay
            }
            Outcome::Save => self.saved(core, s, false),
        }
    }

    /// The form saved, or the error shown next to its field; the namesake
    /// of a player asked about first.
    fn saved(&mut self, core: &Scout4All, s: &Strings, namesake_confirmed: bool) -> Edited {
        self.namesake = None;
        match self.save(core, s, namesake_confirmed) {
            Ok(id) => Edited::Saved(id),
            Err(MobileError::Namesake { reason }) => {
                self.namesake = Some(reason);
                Edited::Stay
            }
            Err(e) => {
                if let Some(field) = self.field_of(&e) {
                    let field = field.min(self.form.fields.len().saturating_sub(1));
                    // the step of the field the error is about
                    match self.kind {
                        Kind::Match(_) => self.show(MatchStep::of(field)),
                        Kind::Season(_) => self.show_season(field >= SEASON_PLAYERS),
                        Kind::Team(_) | Kind::Competition(_) | Kind::Player(_) => {}
                    }
                    self.form.focused = field;
                }
                self.form.error = Some(e.to_string());
                Edited::Stay
            }
        }
    }

    /// To the next step of a match, or the one before when `back`: false
    /// when there's no step to take.
    fn step(&mut self, back: bool) -> bool {
        if let Kind::Season(t) = &self.kind {
            let to = (back == t.on_players).then_some(!back);
            if let Some(players) = to {
                self.show_season(players);
            }
            return to.is_some();
        }
        let Kind::Match(m) = &self.kind else {
            return false;
        };
        let to = match back {
            true => m.step.previous(),
            false => m.step.next(),
        };
        match to {
            Some(step) => {
                self.show(step);
                true
            }
            None => false,
        }
    }

    /// The columns of the score sheet as drawn.
    fn columns(&self) -> usize {
        self.sheet()
            .map_or(ROSTER_COLUMNS, |sheet| sheet.columns.get())
    }

    fn on_roster(&self) -> bool {
        self.sheet().is_some()
    }

    /// A new season: once saved, its team opens.
    pub fn is_season(&self) -> bool {
        matches!(self.kind, Kind::Season(_))
    }

    /// Team A of the match being edited.
    pub fn match_team(&self) -> Option<&str> {
        match &self.kind {
            Kind::Match(m) => Some(&m.team_id),
            _ => None,
        }
    }

    pub fn update(&mut self, core: &Scout4All, s: &Strings, message: form::Message) -> Edited {
        match (&self.namesake, message) {
            (Some(_), form::Message::Save) => self.saved(core, s, true),
            (Some(_), form::Message::Cancel) => {
                self.namesake = None;
                Edited::Stay
            }
            (Some(_), _) => Edited::Stay,
            (None, message) => match (self.correction(), message) {
                (Some(form), message) => {
                    let outcome = form.update(message);
                    self.correction_outcome(core, outcome)
                }
                (None, form::Message::Details(field)) => {
                    self.open_correction(core, s, field);
                    Edited::Stay
                }
                (None, message) => {
                    // a person of the registry taken as the new player
                    if let (form::Message::Take(PLAYER_NAME, n), Kind::Player(_)) =
                        (&message, &self.kind)
                    {
                        let n = *n;
                        let outcome = self.form.update(message);
                        self.take_person(s, n);
                        return self.outcome(core, s, outcome);
                    }
                    let outcome = self.form.update(message);
                    self.outcome(core, s, outcome)
                }
            },
        }
    }

    pub fn key(&mut self, core: &Scout4All, s: &Strings, ck: KeyCombination) -> Edited {
        // the namesake asked about: Enter saves all the same, Esc goes back
        // to the form
        match (&self.namesake, ck) {
            (None, _) => self.form_key(core, s, ck),
            (Some(_), k) if k == key!(enter) => self.saved(core, s, true),
            (Some(_), k) if k == BACK_KEY => {
                self.namesake = None;
                Edited::Stay
            }
            (Some(_), _) => Edited::Stay,
        }
    }

    fn form_key(&mut self, core: &Scout4All, s: &Strings, ck: KeyCombination) -> Edited {
        if let Some(form) = self.correction() {
            let outcome = form.key(ck);
            return self.correction_outcome(core, outcome);
        }
        if self.on_roster() && ck == key!(e) {
            self.open_correction(core, s, self.form.focused);
            return Edited::Stay;
        }
        // the person of the registry Enter or Tab takes from the name
        let taking = match (&self.kind, self.form.offering(), self.form.lit()) {
            (Kind::Player(_), true, Some(lit))
                if self.form.focused == PLAYER_NAME && (ck == key!(enter) || ck == key!(tab)) =>
            {
                Some(lit)
            }
            _ => None,
        };
        if let Some(n) = taking {
            let outcome = self.form.key(ck);
            self.take_person(s, n);
            return self.outcome(core, s, outcome);
        }
        // the score sheet is a grid: the arrows move in it
        let first = self.sheet().map_or(MATCH_ROSTER, |sheet| sheet.first);
        let players = self.form.fields.len().saturating_sub(first);
        let at = self.form.focused.saturating_sub(first);
        let moved = match (self.on_roster(), ck) {
            (false, _) => None,
            (true, k) if k == key!(left) => Some(at.checked_sub(1)),
            (true, k) if k == key!(right) => Some(Some(at + 1)),
            (true, k) if k == key!(up) => Some(at.checked_sub(self.columns())),
            (true, k) if k == key!(down) => Some(Some(at + self.columns())),
            _ => None,
        };
        match moved {
            Some(to) => {
                if let Some(to) = to.filter(|to| *to < players) {
                    self.form.focused = first + to;
                }
                Edited::Stay
            }
            None => {
                let outcome = self.form.key(ck);
                self.outcome(core, s, outcome)
            }
        }
    }

    /// The keys, for the bar at the bottom.
    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        let correcting = self.correcting().is_some();
        let toggle = match self.kind {
            Kind::Season(_) => s.season_toggle,
            _ => s.roster_toggle,
        };
        match (self.namesake.is_some(), self.on_roster()) {
            // the small form of a correction: Enter confirms it
            (false, true) if correcting => Form::hints(s)
                .into_iter()
                .map(|(k, what)| match k.as_str() {
                    "Enter" => (k, s.ok.to_string()),
                    _ => (k, what),
                })
                .collect(),
            (true, _) => vec![
                ("Enter".to_string(), s.save_anyway.to_string()),
                ("Esc".to_string(), s.back.to_string()),
            ],
            (false, true) => vec![
                ("← → ↑ ↓".to_string(), String::new()),
                (s.space.to_string(), toggle.to_string()),
                ("e".to_string(), s.roster_correct.to_string()),
                ("Enter".to_string(), s.save.to_string()),
                ("Esc".to_string(), s.back.to_string()),
            ],
            // the details of a match: Enter goes on to the score sheet
            (false, false) => Form::hints(s)
                .into_iter()
                .map(|(k, what)| match (k.as_str(), &self.kind) {
                    ("Enter", Kind::Match(_) | Kind::Season(_)) => (k, s.go_on.to_string()),
                    _ => (k, what),
                })
                .collect(),
        }
    }

    /// A match not saved yet: once saved, it opens.
    pub fn is_new_match(&self) -> bool {
        matches!(&self.kind, Kind::Match(m) if m.id.is_none())
    }

    pub fn focus_id(&self) -> Id {
        self.correcting()
            .map_or_else(|| self.form.focus_id(), |c| c.form.focus_id())
    }

    pub fn view<'a, M: Clone + 'a>(
        &'a self,
        s: &'static Strings,
        c: Colors,
        on: impl Fn(form::Message) -> M + Copy + 'a,
    ) -> Element<'a, M> {
        match (&self.namesake, &self.kind) {
            (Some(namesake), _) => ui::confirm(
                s,
                s.same_name,
                namesake,
                (theme::icons::SAVE, s.save_anyway),
                on(form::Message::Save),
                on(form::Message::Cancel),
                c,
            ),
            (None, kind) => match (self.correcting(), self.sheet(), kind) {
                // small, over the score sheet
                (Some(correction), _, _) => container(
                    container(correction.form.view(s, c, on, None))
                        .max_width(Self::CORRECTION_WIDTH),
                )
                .center_x(Length::Fill)
                .into(),
                (None, Some(sheet), Kind::Season(_)) => {
                    let chosen = sheet.tiles.iter().filter(|p| p.chosen).count();
                    let count = ui::fill(s.season_count, &[chosen.to_string()]);
                    self.sheet_view(sheet, s.season_players, count, s, c, on)
                }
                (None, Some(sheet), _) => {
                    let chosen = sheet.tiles.iter().filter(|p| p.chosen).count();
                    let count = ui::fill(s.roster_count, &[chosen.to_string()]);
                    self.sheet_view(sheet, s.roster, count, s, c, on)
                }
                (None, None, _) => self.form.view(s, c, on, None),
            },
        }
    }

    /// How wide the small form of a correction is at most.
    const CORRECTION_WIDTH: f32 = 520.0;

    /// How tall a tile of the score sheet is.
    const TILE_HEIGHT: f32 = 80.0;

    /// The score sheet: the players as tiles, by number, the ones on it
    /// lit, as on Android; who played in a set stays.
    fn sheet_view<'a, M: Clone + 'a>(
        &'a self,
        sheet: Sheet<'a>,
        title: &'static str,
        count: String,
        s: &'static Strings,
        c: Colors,
        on: impl Fn(form::Message) -> M + Copy + 'a,
    ) -> Element<'a, M> {
        let Sheet {
            first,
            tiles,
            columns,
        } = sheet;
        // the number and the role of the match, also of who played
        let correct = move |field: usize| -> Element<'a, M> {
            ui::tip(
                button(ui::icon(theme::icons::EDIT, 16.0, c.muted))
                    .padding(4)
                    .on_press(on(form::Message::Details(field)))
                    .style(move |_, status| button::Style {
                        background: matches!(
                            status,
                            button::Status::Hovered | button::Status::Pressed
                        )
                        .then(|| iced::Color { a: 0.7, ..c.button }.into()),
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..iced::Border::default()
                        },
                        ..button::Style::default()
                    }),
                s.roster_correct,
                c,
            )
        };
        let tile = move |n: usize, p: &'a RosterPlayer| -> Element<'a, M> {
            let field = first + n;
            let focused = self.form.focused == field;
            let lit = p.chosen;
            let fg = match lit {
                true => c.text,
                false => c.muted,
            };
            let role: Element<'a, M> = match &p.role {
                Some(role) => text(role.clone())
                    .size(SIZE_SMALL)
                    .color(c.muted)
                    .wrapping(Wrapping::WordOrGlyph)
                    .into(),
                None => space::vertical().height(0).into(),
            };
            button(
                row![
                    text(p.number.to_string())
                        .size(SIZE_TITLE)
                        .font(theme::fonts().bold)
                        .color(match lit {
                            true => c.accent,
                            false => c.muted,
                        })
                        .width(Length::Fixed(34.0)),
                    // the name wraps: the pencil and the check always fit
                    column![
                        text(p.name.clone())
                            .size(SIZE_BODY)
                            .color(fg)
                            .wrapping(Wrapping::WordOrGlyph),
                        role
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                    // the check over the pencil: narrow, the name keeps the room
                    column![
                        text(match lit {
                            true => "■",
                            false => "□",
                        })
                        .size(SIZE_BODY)
                        .color(match (lit, p.played) {
                            (_, true) => c.muted,
                            (true, false) => c.accent,
                            (false, false) => c.border,
                        }),
                        correct(field),
                    ]
                    .spacing(4)
                    .align_x(Alignment::Center),
                ]
                .spacing(8)
                .height(Length::Fill)
                .align_y(Alignment::Center),
            )
            .width(Length::FillPortion(1))
            // all as tall: a name on two lines and the role fit
            .height(Length::Fixed(Self::TILE_HEIGHT))
            .padding(Padding::from([10, 12]))
            // who played stays on the score sheet
            .on_press_maybe(
                (!p.played).then(|| on(form::Message::Choose(field, usize::from(!lit)))),
            )
            .style(move |_, status| {
                let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
                button::Style {
                    background: match (lit, hovered) {
                        (true, _) => Some(
                            iced::Color {
                                a: 0.16,
                                ..c.accent
                            }
                            .into(),
                        ),
                        (false, true) => Some(iced::Color { a: 0.7, ..c.button }.into()),
                        (false, false) => Some(c.background.into()),
                    },
                    text_color: fg,
                    border: iced::Border {
                        color: match (focused, lit) {
                            (true, _) => c.text,
                            (false, true) => c.accent,
                            (false, false) => c.border,
                        },
                        width: match focused {
                            true => 2.0,
                            false => 1.0,
                        },
                        radius: 10.0.into(),
                    },
                    ..button::Style::default()
                }
            })
            .into()
        };
        // as many columns as fit; the arrows go by them
        let grid = responsive(move |size| {
            let fit = roster_columns(size.width);
            columns.set(fit);
            let columns = fit;
            let mut tiles = tiles.iter().enumerate().map(|(n, p)| tile(n, p)).peekable();
            let mut rows: Vec<Element<'a, M>> = vec![];
            while tiles.peek().is_some() {
                let line: Vec<Element<'a, M>> = tiles.by_ref().take(columns).collect();
                let missing = columns - line.len();
                rows.push(
                    Row::with_children(
                        line.into_iter().chain(
                            (0..missing)
                                .map(|_| space::horizontal().width(Length::FillPortion(1)).into()),
                        ),
                    )
                    .spacing(10)
                    .into(),
                );
            }
            scrollable(Column::with_children(rows).spacing(10))
                .height(Length::Fill)
                .into()
        });
        // the grey ones told, when there are
        let legend: Element<'a, M> = match tiles.iter().any(|p| p.played) {
            true => row![
                text("■").size(SIZE_SMALL).color(c.muted),
                text(s.roster_played).size(SIZE_SMALL).color(c.muted),
            ]
            .spacing(6)
            .align_y(Alignment::Center)
            .into(),
            false => space::vertical().height(0).into(),
        };
        let error: Element<'a, M> = match &self.form.error {
            Some(e) => ui::body(e.clone()).color(c.danger).into(),
            None => space::vertical().height(0).into(),
        };
        let buttons = ui::answers(
            Some((theme::icons::CLOSE, s.cancel, on(form::Message::Cancel))),
            theme::icons::CHECK,
            s.save,
            Some(on(form::Message::Save)),
            c,
        );
        let panel = ui::panel(
            Some(title),
            column![
                text(count).size(SIZE_SMALL).color(c.muted),
                legend,
                grid,
                error,
                container(buttons)
                    .width(Length::Fill)
                    .align_x(Alignment::End),
            ]
            .spacing(14),
            c,
        );
        row![
            space::horizontal().width(Length::FillPortion(1)),
            container(panel).width(Length::FillPortion(6)),
            space::horizontal().width(Length::FillPortion(1)),
        ]
        .into()
    }
}

#[cfg(test)]
#[allow(clippy::unreachable)]
mod tests {
    use {
        super::*,
        crate::strings::strings,
        form::Message::{Choose, Input},
        scout4all_core::shapes::{
            country::CountryCode,
            enums::{GenderEnum, LanguageEnum, RoleEnum, TeamClassificationEnum},
        },
        std::sync::Arc,
    };

    fn app(dir: &tempfile::TempDir) -> Arc<Scout4All> {
        Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap()
    }

    /// A team with `players` players, numbered from 1, the seventh the
    /// libero; its id.
    fn team(core: &Scout4All, name: &str, players: u8) -> String {
        let id = core
            .save_team(
                None,
                TeamForm {
                    name: name.into(),
                    classification: Some(TeamClassificationEnum::Regional),
                    gender: Some(GenderEnum::Women),
                    country: CountryCode::known("IT"),
                    year: "2026".into(),
                },
            )
            .unwrap();
        for n in 1..=players {
            let player = PlayerForm {
                name: format!("{name} P{n}"),
                role: Some(match n {
                    1 => RoleEnum::Setter,
                    7 => RoleEnum::Libero,
                    _ => RoleEnum::OutsideHitter,
                }),
                number: n.to_string(),
                ..PlayerForm::default()
            };
            core.save_player(id.clone(), None, player, false).unwrap();
        }
        id
    }

    fn new_match(core: &Scout4All, team_id: &str) -> Editor {
        Editor::a_match(strings(LanguageEnum::It), core, team_id.into(), None).unwrap()
    }

    fn send(e: &mut Editor, core: &Scout4All, message: form::Message) -> Edited {
        e.update(core, strings(LanguageEnum::It), message)
    }

    fn press(e: &mut Editor, core: &Scout4All, ck: KeyCombination) -> Edited {
        e.key(core, strings(LanguageEnum::It), ck)
    }

    fn step(e: &Editor) -> MatchStep {
        match &e.kind {
            Kind::Match(m) => m.step,
            _ => unreachable!("a match"),
        }
    }

    /// The match, a step at a time: Enter goes on, Esc back, and from the
    /// first step cancels.
    #[test]
    fn the_match_is_written_in_three_steps() {
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let cortona = team(&core, "Cortona", 7);
        let mut e = new_match(&core, &cortona);
        assert_eq!(
            e.form.visible(),
            vec![
                MATCH_TEAM,
                MATCH_OPPONENT,
                MATCH_DATE,
                MATCH_TIME,
                MATCH_COURT
            ]
        );
        assert!(matches!(
            send(&mut e, &core, Input(MATCH_OPPONENT, "Arezzo".into())),
            Edited::Stay
        ));
        assert!(matches!(press(&mut e, &core, key!(enter)), Edited::Stay));
        assert_eq!(step(&e), MatchStep::Competition);
        assert_eq!(e.form.focused, MATCH_COMPETITION);
        // no competition written: no format to choose
        assert_eq!(
            e.form.visible(),
            vec![
                MATCH_COMPETITION,
                MATCH_FRIENDLY,
                MATCH_STAGE,
                MATCH_ROUND,
                MATCH_SUBSTITUTIONS,
                MATCH_NOTES
            ]
        );
        press(&mut e, &core, key!(enter));
        assert_eq!(step(&e), MatchStep::Roster);
        assert!(matches!(press(&mut e, &core, BACK_KEY), Edited::Stay));
        assert_eq!(step(&e), MatchStep::Competition);
        press(&mut e, &core, BACK_KEY);
        assert_eq!(step(&e), MatchStep::Match);
        assert!(matches!(press(&mut e, &core, BACK_KEY), Edited::Cancel));
    }

    /// The name of a team links it as team B: nothing to create; a new
    /// name can be made a team, saved without players.
    #[test]
    fn team_b_is_a_team_or_a_name_made_a_team() {
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let cortona = team(&core, "Cortona", 7);
        let cascina = team(&core, "Cascina 2", 0);
        let mut e = new_match(&core, &cortona);
        send(&mut e, &core, Input(MATCH_OPPONENT, "cascina 2".into()));
        assert!(e.form.hidden.contains(&MATCH_CREATE_OPPONENT));
        let Kind::Match(m) = &e.kind else {
            unreachable!("a match")
        };
        assert_eq!(
            e.match_data(m).opponent_team.map(|id| id.to_string()),
            Some(cascina.clone())
        );
        // a name that isn't a team: it may become one; Tab goes through it
        send(&mut e, &core, Input(MATCH_OPPONENT, "Volley Pisa".into()));
        assert!(!e.form.hidden.contains(&MATCH_CREATE_OPPONENT));
        press(&mut e, &core, key!(tab));
        assert_eq!(e.form.focused, MATCH_CREATE_OPPONENT);
        press(&mut e, &core, key!(space));
        let Kind::Match(m) = &e.kind else {
            unreachable!("a match")
        };
        let data = e.match_data(m);
        assert_eq!((data.opponent_team, data.create_opponent), (None, true));
        for _ in 0..3 {
            press(&mut e, &core, key!(enter));
        }
        let teams = core.teams().unwrap();
        let pisa = teams.iter().find_map(|t| match t {
            TeamListing::Team {
                id, name, players, ..
            } if name == "Volley Pisa" => Some((id.clone(), *players)),
            _ => None,
        });
        assert!(matches!(pisa, Some((_, 0))), "{teams:?}");
        // the check left on, the name of a team written: no team created
        let mut e = new_match(&core, &cortona);
        send(&mut e, &core, Input(MATCH_OPPONENT, "Arezzo".into()));
        send(&mut e, &core, Choose(MATCH_CREATE_OPPONENT, 1));
        send(&mut e, &core, Input(MATCH_OPPONENT, "Cascina 2".into()));
        for _ in 0..3 {
            press(&mut e, &core, key!(enter));
        }
        assert_eq!(core.teams().unwrap().len(), 3);
    }

    /// A new competition asks for its format; an existing one doesn't, and
    /// gives its friendly.
    #[test]
    fn the_competition_is_chosen_or_new() {
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let cortona = team(&core, "Cortona", 7);
        let mut e = new_match(&core, &cortona);
        send(&mut e, &core, Input(MATCH_OPPONENT, "Arezzo".into()));
        press(&mut e, &core, key!(enter));
        send(
            &mut e,
            &core,
            Input(MATCH_COMPETITION, "Torneo di Natale".into()),
        );
        assert!(!e.form.hidden.contains(&MATCH_FORMAT));
        // the third format: the tournament
        send(&mut e, &core, Choose(MATCH_FORMAT, 2));
        send(&mut e, &core, Choose(MATCH_STAGE, 2));
        send(&mut e, &core, Input(MATCH_ROUND, "Finale".into()));
        send(&mut e, &core, Choose(MATCH_COURT, COURT_NEUTRAL));
        press(&mut e, &core, key!(enter));
        let Edited::Saved(id) = press(&mut e, &core, key!(enter)) else {
            unreachable!("saved")
        };
        let saved = core.match_form(cortona.clone(), id).unwrap();
        assert_eq!(saved.competition, "Torneo di Natale");
        assert_eq!(
            saved.competition_format,
            Some(scout4all_core::domain::CompetitionFormat::Tournament)
        );
        assert_eq!(saved.stage, Some(scout4all_core::domain::Stage::Pool));
        assert_eq!(saved.round, "Finale");
        assert!(saved.neutral);
        // written again: chosen, no format; its friendly
        let mut e = new_match(&core, &cortona);
        send(&mut e, &core, Input(MATCH_OPPONENT, "Arezzo".into()));
        press(&mut e, &core, key!(enter));
        send(
            &mut e,
            &core,
            Input(MATCH_COMPETITION, "torneo di natale".into()),
        );
        assert!(e.form.hidden.contains(&MATCH_FORMAT));
        press(&mut e, &core, key!(enter));
        press(&mut e, &core, key!(enter));
        assert_eq!(core.competitions().unwrap().len(), 1);
    }

    /// The error of a field takes the keys back to its step.
    #[test]
    fn an_error_goes_back_to_the_step_of_its_field() {
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let cortona = team(&core, "Cortona", 7);
        let mut e = new_match(&core, &cortona);
        press(&mut e, &core, key!(enter));
        press(&mut e, &core, key!(enter));
        assert_eq!(step(&e), MatchStep::Roster);
        assert!(matches!(press(&mut e, &core, key!(enter)), Edited::Stay));
        assert_eq!(step(&e), MatchStep::Match);
        assert_eq!(e.form.focused, MATCH_OPPONENT);
        assert!(e.form.error.is_some());
        // the substitutions: the second step
        send(&mut e, &core, Input(MATCH_OPPONENT, "Arezzo".into()));
        press(&mut e, &core, key!(enter));
        send(&mut e, &core, Input(MATCH_SUBSTITUTIONS, "0".into()));
        press(&mut e, &core, key!(enter));
        press(&mut e, &core, key!(enter));
        assert_eq!(step(&e), MatchStep::Competition);
        assert_eq!(e.form.focused, MATCH_SUBSTITUTIONS);
    }

    /// A new match can be of another team A, with its players, not against
    /// itself; a saved one keeps its team A.
    #[test]
    fn team_a_is_chosen_for_a_new_match_only() {
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let cortona = team(&core, "Cortona", 7);
        let arezzo = team(&core, "Arezzo", 8);
        let mut e = new_match(&core, &cortona);
        let Kind::Match(m) = &e.kind else {
            unreachable!("a match")
        };
        let other = m.teams.iter().position(|(id, _)| *id == arezzo).unwrap();
        send(&mut e, &core, Input(MATCH_OPPONENT, "Arezzo".into()));
        send(&mut e, &core, Choose(MATCH_TEAM, other));
        assert_eq!(e.match_team(), Some(arezzo.as_str()));
        // Arezzo isn't its own opponent: a name, that may become a team
        let Kind::Match(m) = &e.kind else {
            unreachable!("a match")
        };
        assert_eq!(e.match_data(m).opponent_team, None);
        assert_eq!(m.roster.len(), 8);
        send(&mut e, &core, Input(MATCH_OPPONENT, "Cortona".into()));
        press(&mut e, &core, key!(enter));
        press(&mut e, &core, key!(enter));
        let Edited::Saved(id) = press(&mut e, &core, key!(enter)) else {
            unreachable!("saved")
        };
        let saved = core.match_form(arezzo.clone(), id.clone()).unwrap();
        assert_eq!(saved.opponent_team.map(|t| t.to_string()), Some(cortona));
        let edit =
            Editor::a_match(strings(LanguageEnum::It), &core, arezzo.clone(), Some(id)).unwrap();
        let Kind::Match(m) = &edit.kind else {
            unreachable!("a match")
        };
        assert_eq!(m.teams.len(), 1);
    }

    fn correcting(e: &Editor) -> Option<&Correction> {
        match &e.kind {
            Kind::Match(m) => m.correcting.as_ref(),
            _ => unreachable!("a match"),
        }
    }

    fn roster(e: &Editor) -> Vec<(String, u8, Option<RoleEnum>)> {
        match &e.kind {
            Kind::Match(m) => m
                .roster
                .iter()
                .map(|p| (p.name.clone(), p.number, p.role_value))
                .collect(),
            _ => unreachable!("a match"),
        }
    }

    /// On the score sheet, "e" (or the pencil) opens number and role of
    /// the player: a number in use refused there, a good one moves the
    /// player where its number goes, the keys still on it; Esc closes
    /// without changes; saved for the match, not for the team.
    #[test]
    fn number_and_role_are_corrected_on_the_score_sheet() {
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let cortona = team(&core, "Cortona", 7);
        let s = strings(LanguageEnum::It);
        let mut e = new_match(&core, &cortona);
        send(&mut e, &core, Input(MATCH_OPPONENT, "Arezzo".into()));
        press(&mut e, &core, key!(enter));
        press(&mut e, &core, key!(enter));
        assert_eq!(step(&e), MatchStep::Roster);
        assert!(
            e.hints(s)
                .contains(&("e".to_string(), s.roster_correct.to_string()))
        );
        press(&mut e, &core, key!(right));
        press(&mut e, &core, key!(e));
        let c = correcting(&e).unwrap();
        assert_eq!(c.form.title, "2 Cortona P2");
        assert_eq!(c.form.text(CORRECTION_NUMBER), "2");
        assert_eq!(e.focus_id(), Id::from("form-field-0"));
        assert!(
            e.hints(s)
                .contains(&("Enter".to_string(), s.ok.to_string()))
        );
        // in use: refused, the form stays
        send(&mut e, &core, Input(CORRECTION_NUMBER, "3".into()));
        assert!(matches!(press(&mut e, &core, key!(enter)), Edited::Stay));
        assert!(correcting(&e).and_then(|c| c.form.error.clone()).is_some());
        let opposite = core
            .roles(cortona.clone())
            .iter()
            .position(|r| r.value == RoleEnum::OppositeHitter)
            .unwrap();
        send(&mut e, &core, Input(CORRECTION_NUMBER, "12".into()));
        send(&mut e, &core, Choose(CORRECTION_ROLE, opposite + 1));
        press(&mut e, &core, key!(enter));
        assert!(correcting(&e).is_none());
        assert_eq!(
            roster(&e).last(),
            Some(&("Cortona P2".to_string(), 12, Some(RoleEnum::OppositeHitter)))
        );
        assert_eq!(e.form.focused, MATCH_ROSTER + 6);
        // the pencil of the first one, then Esc: nothing changes
        send(&mut e, &core, form::Message::Details(MATCH_ROSTER));
        assert_eq!(
            correcting(&e).map(|c| c.form.title.as_str()),
            Some("1 Cortona P1")
        );
        send(&mut e, &core, Input(CORRECTION_NUMBER, "10".into()));
        assert!(matches!(press(&mut e, &core, BACK_KEY), Edited::Stay));
        assert!(correcting(&e).is_none());
        assert_eq!(step(&e), MatchStep::Roster);
        assert_eq!(roster(&e).first().map(|p| p.1), Some(1));
        let Edited::Saved(id) = press(&mut e, &core, key!(enter)) else {
            unreachable!("saved")
        };
        let edit =
            Editor::a_match(strings(LanguageEnum::It), &core, cortona.clone(), Some(id)).unwrap();
        assert_eq!(
            roster(&edit).last(),
            Some(&("Cortona P2".to_string(), 12, Some(RoleEnum::OppositeHitter)))
        );
        assert_eq!(core.team(cortona).unwrap().players[1].number, 2);
    }

    /// As many tiles as fit, from 2 to 4; the arrows go by the columns
    /// drawn.
    #[test]
    fn the_score_sheet_has_the_columns_that_fit() {
        assert_eq!(roster_columns(0.0), 2);
        assert_eq!(roster_columns(640.0), 2);
        assert_eq!(roster_columns(760.0), 3);
        assert_eq!(roster_columns(1100.0), 4);
        assert_eq!(roster_columns(f32::NAN), 2);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let cortona = team(&core, "Cortona", 9);
        let mut e = new_match(&core, &cortona);
        press(&mut e, &core, key!(enter));
        press(&mut e, &core, key!(enter));
        if let Kind::Match(m) = &e.kind {
            m.columns.set(2);
        }
        press(&mut e, &core, key!(down));
        assert_eq!(e.form.focused, MATCH_ROSTER + 2);
    }

    #[test]
    fn a_date_is_read_where_the_match_is_played() {
        let was = DateTime::parse_from_rfc3339("2026-10-24T18:00:00-03:00").ok();
        let read = parse_date(" 2026-11-02 ", "20:30", was).map(|d| d.to_rfc3339());
        assert_eq!(read.as_deref(), Some("2026-11-02T20:30:00-03:00"));
        assert_eq!(parse_date("2026-11-02", "", was), None);
        assert_eq!(date_text(was, DATE_FORMAT), "2026-10-24");
        assert_eq!(date_text(was, TIME_FORMAT), "18:00");
    }
}
