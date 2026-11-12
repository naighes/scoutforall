//! The forms of a team, of a player and of a match, new or saved: the
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
        widget::{Column, Id, Row, button, column, container, row, scrollable, space, text},
    },
    scout4all_core::{match_form::MatchForm, player_form::PlayerForm, team_form::TeamForm},
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{
        ClassificationChoice, GenderChoice, MatchField, MobileError, PlayerField, RoleChoice,
        RosterPlayer, Scout4All, TeamField,
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

/// The players of the score sheet in a row.
const ROSTER_COLUMNS: usize = 4;

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

pub struct PlayerEditor {
    team_id: String,
    id: Option<String>,
    roles: Vec<RoleChoice>,
}

// the fields of the player
const PLAYER_NAME: usize = 0;
const PLAYER_NUMBER: usize = 1;
const PLAYER_ROLE: usize = 2;
const PLAYER_BIRTH: usize = 3;
const PLAYER_HEIGHT: usize = 4;
const PLAYER_NOTES: usize = 5;

pub struct MatchEditor {
    team_id: String,
    id: Option<String>,
    /// the form as the core keeps it: the roster, the offset of the date
    data: MatchForm,
    roster: Vec<RosterPlayer>,
    /// the teams and the opponents met, offered as the opponent is written
    opponents: Vec<String>,
    /// the second step, as on Android: the players of the score sheet
    on_roster: bool,
}

// the fields of the match, then a check for each player of the roster,
// shown in a step of their own
const MATCH_OPPONENT: usize = 0;
const MATCH_DATE: usize = 1;
const MATCH_TIME: usize = 2;
const MATCH_HOME: usize = 3;
const MATCH_SUBSTITUTIONS: usize = 4;
const MATCH_NOTES: usize = 5;
const MATCH_ROSTER: usize = 6;

pub enum Kind {
    Team(TeamEditor),
    Player(PlayerEditor),
    Match(MatchEditor),
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

impl Editor {
    pub fn team(core: &Scout4All, s: &Strings, id: Option<String>, start: TeamForm) -> Editor {
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
            Field::search(s.country, country).hint(s.search_country),
        ];
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

    pub fn player(
        s: &Strings,
        core: &Scout4All,
        team_id: String,
        id: Option<String>,
        start: PlayerForm,
    ) -> Editor {
        let roles = core.roles();
        let fields = vec![
            Field::text(s.name, start.name.clone()),
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
            kind: Kind::Player(PlayerEditor { team_id, id, roles }),
        }
    }

    /// A new match (no `id`), on the date of now, or a saved one.
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
        let fields = vec![
            Field::search(s.opponent, data.opponent.clone()),
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
                vec![s.home_game.to_string(), s.away_game.to_string()],
                usize::from(!data.home),
            )
            .icons(vec![theme::icons::HOME, theme::icons::AWAY]),
            Field::text(s.max_substitutions, data.max_substitutions.clone()),
            Field::text(s.notes, data.notes.clone()),
        ];
        let title = match id {
            Some(_) => s.edit_match,
            None => s.new_match,
        };
        let mut form = Form::new(title, fields);
        form.shown = Some(0..MATCH_ROSTER);
        form.actions = Some((
            (theme::icons::CLOSE, s.cancel.to_string()),
            (theme::icons::FORWARD_ARROW, s.go_on.to_string()),
        ));
        let opponents = core.opponents(team_id.clone()).unwrap_or_default();
        let mut editor = Editor {
            form,
            namesake: None,
            kind: Kind::Match(MatchEditor {
                team_id,
                id,
                data,
                roster: vec![],
                opponents,
                on_roster: false,
            }),
        };
        editor.toggle(core, None)?;
        Ok(editor)
    }

    /// The match form as written, the roster as the core keeps it.
    fn match_data(&self, m: &MatchEditor) -> MatchForm {
        let f = &self.form;
        MatchForm {
            opponent: f.text(MATCH_OPPONENT),
            date: parse_date(&f.text(MATCH_DATE), &f.text(MATCH_TIME), m.data.date),
            home: f.index(MATCH_HOME) == 0,
            max_substitutions: f.text(MATCH_SUBSTITUTIONS),
            notes: f.text(MATCH_NOTES),
            roster: m.data.roster.clone(),
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
        Ok(())
    }

    fn changed(&mut self, core: &Scout4All, field: usize) {
        match (&mut self.kind, field) {
            (Kind::Team(_), TEAM_COUNTRY) => {
                let written = self.form.text(TEAM_COUNTRY);
                self.form
                    .suggest(TEAM_COUNTRY, countries::suggestions(core, &written));
            }
            (Kind::Match(m), MATCH_OPPONENT) => {
                let offered = scout4all_mobile::offered_names(
                    m.opponents.clone(),
                    self.form.text(MATCH_OPPONENT),
                )
                .into_iter()
                .map(|o| Suggestion {
                    label: o.clone(),
                    value: o,
                })
                .collect();
                self.form.suggest(MATCH_OPPONENT, offered);
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
                let country = countries::chosen(core, &f.text(TEAM_COUNTRY)).map_err(|()| {
                    MobileError::InvalidTeam {
                        field: TeamField::Country,
                        reason: s.no_country_found.to_string(),
                    }
                })?;
                let form = TeamForm {
                    name: f.text(TEAM_NAME),
                    year: f.text(TEAM_YEAR),
                    classification: f
                        .index(TEAM_CLASSIFICATION)
                        .checked_sub(1)
                        .and_then(|i| t.classifications.get(i))
                        .map(|c| c.value),
                    gender: f
                        .index(TEAM_GENDER)
                        .checked_sub(1)
                        .and_then(|i| t.genders.get(i))
                        .map(|g| g.value),
                    country,
                };
                core.save_team(t.id.clone(), form)
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
            MobileError::InvalidPlayer { field, .. } => Some(match field {
                PlayerField::Name => PLAYER_NAME,
                PlayerField::Number => PLAYER_NUMBER,
                PlayerField::Role => PLAYER_ROLE,
                PlayerField::BirthDate => PLAYER_BIRTH,
                PlayerField::Height => PLAYER_HEIGHT,
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
                self.changed(core, field);
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
                    // the details, if the error is about them
                    if field < MATCH_ROSTER {
                        self.step(true);
                    }
                    self.form.focused = field;
                }
                self.form.error = Some(e.to_string());
                Edited::Stay
            }
        }
    }

    /// From the details of a match to its score sheet, or `back`: false
    /// when there's no step to take.
    fn step(&mut self, back: bool) -> bool {
        let Kind::Match(m) = &mut self.kind else {
            return false;
        };
        match (m.on_roster, back) {
            (false, false) => {
                m.on_roster = true;
                self.form.shown = Some(MATCH_ROSTER..self.form.fields.len());
                self.form.focused = MATCH_ROSTER;
                true
            }
            (true, true) => {
                m.on_roster = false;
                self.form.shown = Some(0..MATCH_ROSTER);
                self.form.focused = MATCH_OPPONENT;
                true
            }
            _ => false,
        }
    }

    fn on_roster(&self) -> bool {
        matches!(&self.kind, Kind::Match(m) if m.on_roster)
    }

    pub fn update(&mut self, core: &Scout4All, s: &Strings, message: form::Message) -> Edited {
        match (&self.namesake, message) {
            (Some(_), form::Message::Save) => self.saved(core, s, true),
            (Some(_), form::Message::Cancel) => {
                self.namesake = None;
                Edited::Stay
            }
            (Some(_), _) => Edited::Stay,
            (None, message) => {
                let outcome = self.form.update(message);
                self.outcome(core, s, outcome)
            }
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
        // the score sheet is a grid: the arrows move in it
        let players = self.form.fields.len().saturating_sub(MATCH_ROSTER);
        let at = self.form.focused.saturating_sub(MATCH_ROSTER);
        let moved = match (self.on_roster(), ck) {
            (false, _) => None,
            (true, k) if k == key!(left) => Some(at.checked_sub(1)),
            (true, k) if k == key!(right) => Some(Some(at + 1)),
            (true, k) if k == key!(up) => Some(at.checked_sub(ROSTER_COLUMNS)),
            (true, k) if k == key!(down) => Some(Some(at + ROSTER_COLUMNS)),
            _ => None,
        };
        match moved {
            Some(to) => {
                if let Some(to) = to.filter(|to| *to < players) {
                    self.form.focused = MATCH_ROSTER + to;
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
        match (self.namesake.is_some(), self.on_roster()) {
            (true, _) => vec![
                ("Enter".to_string(), s.save_anyway.to_string()),
                ("Esc".to_string(), s.back.to_string()),
            ],
            (false, true) => vec![
                ("← → ↑ ↓".to_string(), String::new()),
                (s.space.to_string(), s.roster_toggle.to_string()),
                ("Enter".to_string(), s.save.to_string()),
                ("Esc".to_string(), s.back.to_string()),
            ],
            // the details of a match: Enter goes on to the score sheet
            (false, false) => Form::hints(s)
                .into_iter()
                .map(|(k, what)| match (k.as_str(), &self.kind) {
                    ("Enter", Kind::Match(_)) => (k, s.go_on.to_string()),
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
        self.form.focus_id()
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
            (None, Kind::Match(m)) if m.on_roster => self.roster_view(m, s, c, on),
            (None, _) => self.form.view(s, c, on, None),
        }
    }

    /// How tall a tile of the score sheet is.
    const TILE_HEIGHT: f32 = 80.0;

    /// The score sheet: the players as tiles, by number, the ones on it
    /// lit, as on Android; who played in a set stays.
    fn roster_view<'a, M: Clone + 'a>(
        &'a self,
        m: &'a MatchEditor,
        s: &'static Strings,
        c: Colors,
        on: impl Fn(form::Message) -> M + Copy + 'a,
    ) -> Element<'a, M> {
        let chosen = m.roster.iter().filter(|p| p.chosen).count();
        let tile = |n: usize, p: &'a RosterPlayer| -> Element<'a, M> {
            let field = MATCH_ROSTER + n;
            let focused = self.form.focused == field;
            let lit = p.chosen;
            let fg = match lit {
                true => c.text,
                false => c.muted,
            };
            let role: Element<'a, M> = match &p.role {
                Some(role) => text(role.clone()).size(SIZE_SMALL).color(c.muted).into(),
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
                    column![text(p.name.clone()).size(SIZE_BODY).color(fg), role].spacing(2),
                    space::horizontal(),
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
        let tiles: Vec<Element<'a, M>> = m
            .roster
            .iter()
            .enumerate()
            .map(|(n, p)| tile(n, p))
            .collect();
        let mut rows: Vec<Element<'a, M>> = vec![];
        let mut tiles = tiles.into_iter().peekable();
        while tiles.peek().is_some() {
            let line: Vec<Element<'a, M>> = tiles.by_ref().take(ROSTER_COLUMNS).collect();
            let missing = ROSTER_COLUMNS - line.len();
            rows.push(
                Row::with_children(line.into_iter().chain(
                    (0..missing).map(|_| space::horizontal().width(Length::FillPortion(1)).into()),
                ))
                .spacing(10)
                .into(),
            );
        }
        // the grey ones told, when there are
        let legend: Element<'a, M> = match m.roster.iter().any(|p| p.played) {
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
            Some(s.roster),
            column![
                text(ui::fill(s.roster_count, &[chosen.to_string()]))
                    .size(SIZE_SMALL)
                    .color(c.muted),
                legend,
                scrollable(Column::with_children(rows).spacing(10)).height(Length::Fill),
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
mod tests {
    use super::*;

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
