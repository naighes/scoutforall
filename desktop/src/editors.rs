//! The forms of a team, of a player and of a match, new or saved: the
//! fields of `form`, saved through the core, which checks them. An error
//! takes the keys to its field.

use {
    crate::{
        form::{self, Field, Form, Outcome},
        strings::Strings,
        theme::Colors,
    },
    chrono::{DateTime, FixedOffset, Local, NaiveDateTime, TimeZone},
    iced::{Element, widget::Id},
    scout4all_core::{
        match_form::MatchForm, player_form::PlayerForm, shapes::country::CountryCode,
        team_form::TeamForm,
    },
    scout4all_keys::KeyCombination,
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

const DATE_FORMAT: &str = "%Y-%m-%d %H:%M";

/// The first option of a choice that can be left empty.
const EMPTY: &str = "–";

pub struct TeamEditor {
    id: Option<String>,
    classifications: Vec<ClassificationChoice>,
    genders: Vec<GenderChoice>,
    /// the countries the search finds, offered by the last field
    countries: Vec<(CountryCode, String)>,
}

// the fields of the team
const TEAM_NAME: usize = 0;
const TEAM_YEAR: usize = 1;
const TEAM_CLASSIFICATION: usize = 2;
const TEAM_GENDER: usize = 3;
const TEAM_SEARCH: usize = 4;
const TEAM_COUNTRY: usize = 5;

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
}

// the fields of the match, then a check for each player of the roster
const MATCH_OPPONENT: usize = 0;
const MATCH_DATE: usize = 1;
const MATCH_HOME: usize = 2;
const MATCH_SUBSTITUTIONS: usize = 3;
const MATCH_NOTES: usize = 4;
const MATCH_ROSTER: usize = 5;

pub enum Kind {
    Team(TeamEditor),
    Player(PlayerEditor),
    Match(MatchEditor),
}

pub struct Editor {
    pub form: Form,
    kind: Kind,
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

fn roster_label(p: &RosterPlayer) -> String {
    match &p.role {
        Some(role) => format!("{:>2}  {} · {role}", p.number, p.name),
        None => format!("{:>2}  {}", p.number, p.name),
    }
}

fn date_text(date: Option<DateTime<FixedOffset>>) -> String {
    date.map(|d| d.format(DATE_FORMAT).to_string())
        .unwrap_or_default()
}

/// The date written, at the offset of the one it replaces (where the match
/// is played), else at the one of here; none if it can't be read.
fn parse_date(text: &str, was: Option<DateTime<FixedOffset>>) -> Option<DateTime<FixedOffset>> {
    let naive = NaiveDateTime::parse_from_str(text.trim(), DATE_FORMAT).ok()?;
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
        let country = start.country.map(|c| {
            let name = core.country_name(c);
            (c, name)
        });
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
            Field::text(
                s.country,
                country.as_ref().map(|(_, n)| n.clone()).unwrap_or_default(),
            ),
            Field::choice("", vec![], 0),
        ];
        let title = match id {
            Some(_) => s.edit_team,
            None => s.new_team,
        };
        let mut editor = Editor {
            form: Form::new(title, fields),
            kind: Kind::Team(TeamEditor {
                id,
                classifications,
                genders,
                countries: country.into_iter().collect(),
            }),
        };
        editor.offer_countries(s);
        editor
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
            Field::text(s.birth_date, start.birth_date.clone()),
            Field::text(s.height, start.height_cm.clone()),
            Field::text(s.notes, start.notes.clone()),
        ];
        let title = match id {
            Some(_) => s.edit_player,
            None => s.new_player,
        };
        Editor {
            form: Form::new(title, fields),
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
            Field::text(s.opponent, data.opponent.clone()),
            Field::text(s.date, date_text(data.date)).hint(s.date_format),
            Field::choice(
                &format!("{} / {}", s.home_game, s.away_game),
                vec![s.home_game.to_string(), s.away_game.to_string()],
                usize::from(!data.home),
            ),
            Field::text(s.max_substitutions, data.max_substitutions.clone()),
            Field::text(s.notes, data.notes.clone()),
        ];
        let title = match id {
            Some(_) => s.edit_match,
            None => s.new_match,
        };
        let mut form = Form::new(title, fields);
        form.sections = vec![(MATCH_ROSTER, s.roster.to_string())];
        let mut editor = Editor {
            form,
            kind: Kind::Match(MatchEditor {
                team_id,
                id,
                data,
                roster: vec![],
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
            date: parse_date(&f.text(MATCH_DATE), m.data.date),
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
                .map(|p| Field::check(&roster_label(p), p.chosen, p.played)),
        );
        if let Kind::Match(m) = &mut self.kind {
            m.data = roster.form;
            m.roster = roster.players;
        }
        Ok(())
    }

    /// The countries found by the search, the one chosen first.
    fn offer_countries(&mut self, s: &Strings) {
        let Kind::Team(t) = &self.kind else {
            return;
        };
        let options = match t.countries.is_empty() {
            true => vec![s.no_country_found.to_string()],
            false => t.countries.iter().map(|(_, name)| name.clone()).collect(),
        };
        if let Some(field) = self.form.fields.get_mut(TEAM_COUNTRY) {
            field.value = form::Value::Choice { options, index: 0 };
        }
    }

    fn changed(&mut self, core: &Scout4All, s: &Strings, field: usize) {
        match (&mut self.kind, field) {
            (Kind::Team(t), TEAM_SEARCH) => {
                t.countries = core
                    .countries(self.form.text(TEAM_SEARCH))
                    .into_iter()
                    .take(12)
                    .map(|c| (c.value, c.name))
                    .collect();
                self.offer_countries(s);
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

    fn save(&mut self, core: &Scout4All) -> Result<String, MobileError> {
        let f = &self.form;
        match &self.kind {
            Kind::Team(t) => {
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
                    country: t.countries.get(f.index(TEAM_COUNTRY)).map(|(c, _)| *c),
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
                core.save_player(p.team_id.clone(), p.id.clone(), form)
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
                TeamField::Country => TEAM_SEARCH,
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
            Outcome::Cancel => Edited::Cancel,
            Outcome::Changed(field) => {
                self.changed(core, s, field);
                Edited::Stay
            }
            Outcome::Save => match self.save(core) {
                Ok(id) => Edited::Saved(id),
                Err(e) => {
                    if let Some(field) = self.field_of(&e) {
                        self.form.focused = field.min(self.form.fields.len().saturating_sub(1));
                    }
                    self.form.error = Some(e.to_string());
                    Edited::Stay
                }
            },
        }
    }

    pub fn update(&mut self, core: &Scout4All, s: &Strings, message: form::Message) -> Edited {
        let outcome = self.form.update(message);
        self.outcome(core, s, outcome)
    }

    pub fn key(&mut self, core: &Scout4All, s: &Strings, ck: KeyCombination) -> Edited {
        let outcome = self.form.key(ck);
        self.outcome(core, s, outcome)
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
        self.form.view(s, c, on, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_date_is_read_where_the_match_is_played() {
        let was = DateTime::parse_from_rfc3339("2026-10-24T18:00:00-03:00").ok();
        let read = parse_date(" 2026-11-02 20:30 ", was).map(|d| d.to_rfc3339());
        assert_eq!(read.as_deref(), Some("2026-11-02T20:30:00-03:00"));
        assert_eq!(parse_date("2 novembre", was), None);
        assert_eq!(date_text(was), "2026-10-24 18:00");
    }
}
