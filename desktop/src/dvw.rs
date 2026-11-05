//! The import of a DataVolley file, in the steps of the TUI and of the
//! app: the teams to import, the day when the file can't tell it, the
//! substitutions, then for each team ours (a similar one or a new one),
//! the check, the save; at the end what was imported. Each step is a form:
//! Enter goes on, Esc goes back; on the check Esc leaves the team as it
//! is.

use {
    crate::{
        form::{self, Field, Form},
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL},
        ui,
    },
    chrono::NaiveDate,
    iced::{
        Alignment, Element, Length,
        widget::{Column, Id, column, container, row, text},
    },
    scout4all_core::shapes::country::CountryCode,
    scout4all_keys::{BACK_KEY, KeyCombination, key},
    scout4all_mobile::{
        ClassificationChoice, DvwChoice, DvwPreview, DvwReady, DvwSide, DvwSimilarTeam,
        GenderChoice, MobileError, Scout4All, TeamField,
    },
    std::sync::Arc,
};

enum Step {
    Sides,
    Day,
    Substitutions,
    /// ours like the one of the file, and a new one last
    Team(Vec<DvwSimilarTeam>),
    NewTeam(Vec<DvwSimilarTeam>),
    Ready(DvwReady, DvwChoice),
    Done,
}

#[derive(Debug, Clone)]
pub enum Message {
    Form(form::Message),
    Save,
    Skip,
    Open(usize),
    Close,
}

pub enum Outcome {
    Stay,
    /// back to the home
    Back,
    Team(String),
}

/// What happened to a team of the file: imported (its id) or why not.
struct TeamResult {
    message: String,
    team_id: Option<String>,
}

// the fields of the new team
const CLASSIFICATION: usize = 0;
const GENDER: usize = 1;
const SEARCH: usize = 2;
const COUNTRY: usize = 3;

/// The first option of a choice that can be left empty.
const EMPTY: &str = "–";

pub struct DvwScreen {
    core: Arc<Scout4All>,
    /// the team the import was asked from: offered first, back to it
    pub from: Option<String>,
    bytes: Arc<Vec<u8>>,
    name: String,
    preview: DvwPreview,
    step: Step,
    form: Form,
    sides: Vec<DvwSide>,
    /// the side being imported, of `sides`
    current: usize,
    /// `YYYY-MM-DD`
    day: String,
    substitutions: u8,
    classifications: Vec<ClassificationChoice>,
    genders: Vec<GenderChoice>,
    /// the countries the search finds; the one of the settings to start
    countries: Vec<(CountryCode, String)>,
    results: Vec<TeamResult>,
    selected: usize,
}

impl DvwScreen {
    pub fn new(
        core: Arc<Scout4All>,
        s: &Strings,
        name: String,
        bytes: Arc<Vec<u8>>,
        from: Option<String>,
    ) -> Result<DvwScreen, MobileError> {
        let preview = core.preview_dvw(bytes.to_vec())?;
        let year = chrono::Datelike::year(&chrono::Local::now());
        let country = core
            .new_team_form(u16::try_from(year).unwrap_or(2026))
            .country
            .map(|c| (c, core.country_name(c)));
        let mut screen = DvwScreen {
            classifications: core.classifications(),
            genders: core.genders(),
            core,
            from,
            bytes,
            name,
            day: preview.days.first().cloned().unwrap_or_default(),
            preview,
            step: Step::Sides,
            form: Form::new("", vec![]),
            sides: vec![],
            current: 0,
            substitutions: 0,
            countries: country.into_iter().collect(),
            results: vec![],
            selected: 0,
        };
        screen.to(s, Step::Sides);
        Ok(screen)
    }

    pub fn focus_id(&self) -> Option<Id> {
        match self.step {
            Step::Ready(..) | Step::Done => None,
            _ => Some(self.form.focus_id()),
        }
    }

    fn team_name(&self) -> String {
        match self.sides.get(self.current) {
            Some(DvwSide::Home) => self.preview.home.clone(),
            Some(DvwSide::Visiting) => self.preview.visiting.clone(),
            None => String::new(),
        }
    }

    /// The step, with the form to answer it.
    fn to(&mut self, s: &Strings, step: Step) {
        let team = self.team_name();
        let (fields, section) = match &step {
            Step::Sides => (
                vec![Field::choice(
                    s.dvw_which_team,
                    vec![
                        ui::fill(s.dvw_home, std::slice::from_ref(&self.preview.home)),
                        ui::fill(s.dvw_visiting, std::slice::from_ref(&self.preview.visiting)),
                        s.dvw_both.to_string(),
                    ],
                    self.side_of_from(),
                )],
                None,
            ),
            Step::Day => match self.preview.days.as_slice() {
                [_, _] => (
                    vec![Field::choice(
                        s.dvw_which_date,
                        self.preview.days.clone(),
                        0,
                    )],
                    None,
                ),
                _ => (
                    vec![Field::text(s.dvw_no_date, self.day.clone()).hint(s.dvw_day_hint)],
                    None,
                ),
            },
            Step::Substitutions => (
                vec![
                    Field::text(s.max_substitutions, self.substitutions.to_string())
                        .hint(s.dvw_substitutions),
                ],
                None,
            ),
            Step::Team(similar) => (
                vec![Field::choice(
                    &ui::fill(s.dvw_similar_found, &[team]),
                    similar
                        .iter()
                        .map(|t| similar_label(s, t))
                        .chain([s.dvw_new_team.to_string()])
                        .collect(),
                    0,
                )],
                None,
            ),
            Step::NewTeam(_) => {
                let season = self
                    .preview
                    .season
                    .map(|y| y.to_string())
                    .unwrap_or_default();
                (
                    vec![
                        Field::choice(
                            s.classification,
                            with_empty(self.classifications.iter().map(|c| c.name.clone())),
                            0,
                        ),
                        Field::choice(
                            s.gender,
                            with_empty(self.genders.iter().map(|g| g.name.clone())),
                            0,
                        ),
                        Field::text(
                            s.country,
                            self.countries
                                .first()
                                .map(|(_, n)| n.clone())
                                .unwrap_or_default(),
                        ),
                        Field::choice("", vec![], 0),
                    ],
                    Some(ui::fill(s.dvw_new_team_of, &[team, season])),
                )
            }
            Step::Ready(..) | Step::Done => (vec![], None),
        };
        let mut form = Form::new(s.import_dvw, fields);
        form.sections = section.map(|t| (0, t)).into_iter().collect();
        form.actions = Some((s.back.to_string(), s.dvw_next.to_string()));
        self.form = form;
        self.step = step;
        self.selected = 0;
        self.offer_countries(s);
    }

    /// The countries found by the search, the one chosen first.
    fn offer_countries(&mut self, s: &Strings) {
        if let (Step::NewTeam(_), Some(field)) = (&self.step, self.form.fields.get_mut(COUNTRY)) {
            let options = match self.countries.is_empty() {
                true => vec![s.no_country_found.to_string()],
                false => self.countries.iter().map(|(_, n)| n.clone()).collect(),
            };
            field.value = form::Value::Choice { options, index: 0 };
        }
    }

    fn changed(&mut self, s: &Strings, field: usize) {
        if let (Step::NewTeam(_), SEARCH) = (&self.step, field) {
            self.countries = self
                .core
                .countries(self.form.text(SEARCH))
                .into_iter()
                .take(12)
                .map(|c| (c.value, c.name))
                .collect();
            self.offer_countries(s);
        }
    }

    fn choice(&self, team_id: Option<String>) -> DvwChoice {
        let new = matches!(self.step, Step::NewTeam(_));
        let f = &self.form;
        DvwChoice {
            side: self
                .sides
                .get(self.current)
                .copied()
                .unwrap_or(DvwSide::Home),
            day: self.day.clone(),
            max_substitutions: self.substitutions,
            team_id,
            classification: new
                .then(|| f.index(CLASSIFICATION).checked_sub(1))
                .flatten()
                .and_then(|i| self.classifications.get(i))
                .map(|c| c.value),
            gender: new
                .then(|| f.index(GENDER).checked_sub(1))
                .flatten()
                .and_then(|i| self.genders.get(i))
                .map(|g| g.value),
            country: new
                .then(|| self.countries.get(f.index(COUNTRY)))
                .flatten()
                .map(|(c, _)| *c),
        }
    }

    /// On to the next step with what's chosen.
    fn next(&mut self, s: &Strings) {
        self.form.error = None;
        let step = std::mem::replace(&mut self.step, Step::Done);
        match step {
            Step::Sides => {
                self.sides = match self.form.index(0) {
                    0 => vec![DvwSide::Home],
                    1 => vec![DvwSide::Visiting],
                    _ => vec![DvwSide::Home, DvwSide::Visiting],
                };
                self.current = 0;
                self.results = vec![];
                match self.preview.days.as_slice() {
                    [day] => {
                        self.day = day.clone();
                        self.substitutions_of_the_day(s);
                    }
                    _ => self.to(s, Step::Day),
                }
            }
            Step::Day => {
                let day = match self.preview.days.as_slice() {
                    [_, _] => self.preview.days.get(self.form.index(0)).cloned(),
                    _ => Some(self.form.text(0).trim().to_string())
                        .filter(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok()),
                };
                match day {
                    Some(day) => {
                        self.day = day;
                        self.substitutions_of_the_day(s);
                    }
                    None => {
                        self.step = Step::Day;
                        self.form.error = Some(format!("{}: {}", s.date, s.dvw_day_hint));
                    }
                }
            }
            Step::Substitutions => match self.form.text(0).trim().parse::<u8>() {
                Ok(n) if n > 0 => {
                    self.substitutions = n;
                    self.team(s);
                }
                _ => {
                    self.step = Step::Substitutions;
                    self.form.error = Some(s.dvw_substitutions.to_string());
                }
            },
            Step::Team(similar) => match similar.get(self.form.index(0)) {
                Some(team) => {
                    let id = team.id.clone();
                    self.step = Step::Team(similar);
                    self.plan(s, Some(id));
                }
                None => self.to(s, Step::NewTeam(similar)),
            },
            Step::NewTeam(similar) => {
                self.step = Step::NewTeam(similar);
                self.plan(s, None);
            }
            Step::Ready(ready, choice) => self.save(s, &ready, &choice),
            Step::Done => {}
        }
    }

    /// Back a step: false before the first one.
    fn back(&mut self, s: &Strings) -> bool {
        let step = std::mem::replace(&mut self.step, Step::Done);
        match step {
            Step::Sides => {
                self.step = Step::Sides;
                return false;
            }
            Step::Done => return false,
            Step::Day => self.to(s, Step::Sides),
            Step::Substitutions => match self.preview.days.len() {
                1 => self.to(s, Step::Sides),
                _ => self.to(s, Step::Day),
            },
            Step::Team(_) => self.to(s, Step::Substitutions),
            Step::NewTeam(similar) => match similar.is_empty() {
                true => self.to(s, Step::Substitutions),
                false => self.to(s, Step::Team(similar)),
            },
            Step::Ready(ready, _) => {
                let message = ui::fill(s.dvw_skipped, &[ready.team]);
                self.next_side(s, message, None);
            }
        }
        true
    }

    fn substitutions_of_the_day(&mut self, s: &Strings) {
        self.substitutions = self.core.dvw_max_substitutions(Some(self.day.clone()));
        self.to(s, Step::Substitutions);
    }

    /// The side whose team is like the one the import was asked from, of
    /// the choices of the sides: the home one when neither.
    fn side_of_from(&self) -> usize {
        let like = |side| {
            self.from.as_ref().is_some_and(|from| {
                self.core
                    .dvw_similar_teams(self.bytes.to_vec(), side, None)
                    .is_ok_and(|similar| similar.iter().any(|t| t.id == *from))
            })
        };
        match (like(DvwSide::Home), like(DvwSide::Visiting)) {
            (false, true) => 1,
            _ => 0,
        }
    }

    /// The team of the side being imported: a similar one or a new one;
    /// done after the last side. The team the import was asked from comes
    /// first, until a side goes into it.
    fn team(&mut self, s: &Strings) {
        let Some(side) = self.sides.get(self.current).copied() else {
            return self.to(s, Step::Done);
        };
        let first = self.from.clone().filter(|from| {
            !self
                .results
                .iter()
                .any(|r| r.team_id.as_ref() == Some(from))
        });
        match self
            .core
            .dvw_similar_teams(self.bytes.to_vec(), side, first)
        {
            Ok(similar) if similar.is_empty() => self.to(s, Step::NewTeam(similar)),
            Ok(similar) => self.to(s, Step::Team(similar)),
            Err(e) => {
                let message = ui::fill(s.dvw_not_imported, &[self.team_name(), e.to_string()]);
                self.next_side(s, message, None);
            }
        }
    }

    fn plan(&mut self, s: &Strings, team_id: Option<String>) {
        let choice = self.choice(team_id);
        match self.core.plan_dvw(self.bytes.to_vec(), choice.clone()) {
            Ok(ready) => self.to(s, Step::Ready(ready, choice)),
            // the new team still to fill in, as in the form
            Err(MobileError::InvalidTeam { field, reason }) => {
                self.form.focused = match field {
                    TeamField::Gender => GENDER,
                    TeamField::Country => SEARCH,
                    _ => CLASSIFICATION,
                };
                self.form.error = Some(reason);
            }
            Err(e) => {
                let message = ui::fill(s.dvw_not_imported, &[self.team_name(), e.to_string()]);
                self.next_side(s, message, None);
            }
        }
    }

    fn save(&mut self, s: &Strings, ready: &DvwReady, choice: &DvwChoice) {
        let imported =
            self.core
                .import_dvw(self.bytes.to_vec(), choice.clone(), ready.saved_already);
        let team = ready.team.clone();
        match imported {
            Ok(imported) => {
                self.next_side(s, ui::fill(s.dvw_imported, &[team]), Some(imported.team_id))
            }
            Err(e) => self.next_side(
                s,
                ui::fill(s.dvw_not_imported, &[team, e.to_string()]),
                None,
            ),
        }
    }

    fn next_side(&mut self, s: &Strings, message: String, team_id: Option<String>) {
        self.results.push(TeamResult { message, team_id });
        self.current += 1;
        self.team(s);
    }

    /// The team of the result, once imported.
    fn open(&self, index: usize) -> Outcome {
        self.results
            .get(index)
            .and_then(|r| r.team_id.clone())
            .map_or(Outcome::Stay, Outcome::Team)
    }

    fn form_outcome(&mut self, s: &Strings, outcome: form::Outcome) -> Outcome {
        match outcome {
            form::Outcome::Stay => Outcome::Stay,
            form::Outcome::Changed(field) => {
                self.changed(s, field);
                Outcome::Stay
            }
            form::Outcome::Save => {
                self.next(s);
                Outcome::Stay
            }
            form::Outcome::Cancel => match self.back(s) {
                true => Outcome::Stay,
                false => Outcome::Back,
            },
        }
    }

    pub fn key(&mut self, s: &Strings, ck: KeyCombination) -> Outcome {
        let count = self.results.len();
        match (&self.step, ck) {
            (Step::Ready(..), k) if k == key!(enter) => {
                self.next(s);
                Outcome::Stay
            }
            (Step::Ready(..), k) if k == BACK_KEY => {
                self.back(s);
                Outcome::Stay
            }
            (Step::Ready(..), _) => Outcome::Stay,
            (Step::Done, k) if k == BACK_KEY => Outcome::Back,
            (Step::Done, k) if k == key!(enter) => self.open(self.selected),
            (Step::Done, k) if k == key!(down) && count > 0 => {
                self.selected = (self.selected + 1) % count;
                Outcome::Stay
            }
            (Step::Done, k) if k == key!(up) && count > 0 => {
                self.selected = (self.selected + count - 1) % count;
                Outcome::Stay
            }
            (Step::Done, _) => Outcome::Stay,
            (_, ck) => {
                let outcome = self.form.key(ck);
                self.form_outcome(s, outcome)
            }
        }
    }

    pub fn update(&mut self, s: &Strings, message: Message) -> Outcome {
        match message {
            Message::Form(m) => {
                let outcome = self.form.update(m);
                self.form_outcome(s, outcome)
            }
            Message::Save => {
                self.next(s);
                Outcome::Stay
            }
            Message::Skip => {
                self.back(s);
                Outcome::Stay
            }
            Message::Open(i) => {
                self.selected = i;
                self.open(i)
            }
            Message::Close => Outcome::Back,
        }
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        match &self.step {
            Step::Ready(ready, _) => vec![
                (
                    "Enter".to_string(),
                    match ready.saved_already {
                        true => s.dvw_overwrite,
                        false => s.save,
                    }
                    .to_string(),
                ),
                ("Esc".to_string(), s.dvw_dont_import.to_string()),
            ],
            Step::Done => vec![
                ("↑ ↓".to_string(), String::new()),
                ("Enter".to_string(), s.open.to_string()),
                ("Esc".to_string(), s.back.to_string()),
            ],
            _ => vec![
                ("↑ ⇧⇥".to_string(), s.previous.to_string()),
                ("↓ ⇥".to_string(), s.next.to_string()),
                ("← →".to_string(), s.change.to_string()),
                ("Enter".to_string(), s.dvw_next.to_string()),
                ("Esc".to_string(), s.back.to_string()),
            ],
        }
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let summary = self.summary(s, c);
        match &self.step {
            Step::Ready(ready, _) => {
                let warning: Element<'a, Message> = match ready.saved_already {
                    true => ui::body(ui::fill(
                        s.dvw_already_saved,
                        std::slice::from_ref(&ready.team),
                    ))
                    .color(c.danger)
                    .into(),
                    false => column![].into(),
                };
                let yes = match ready.saved_already {
                    true => s.dvw_overwrite,
                    false => s.save,
                };
                side_by_side(
                    ui::panel(
                        Some(s.import_dvw),
                        column![
                            ui::body(ui::fill(
                                s.dvw_ready,
                                &[
                                    ready.team.clone(),
                                    ready.sets.to_string(),
                                    ready.new_players.to_string()
                                ]
                            )),
                            warning,
                            buttons(
                                &format!("Esc  {}", s.dvw_dont_import),
                                Message::Skip,
                                &format!("Enter  {yes}"),
                                Message::Save,
                                c
                            ),
                        ]
                        .spacing(16),
                        c,
                    ),
                    summary,
                )
            }
            Step::Done => {
                let rows = self.results.iter().enumerate().map(|(i, r)| {
                    ui::list_row(
                        ui::body(r.message.clone()),
                        i == self.selected,
                        r.team_id.as_ref().map(|_| Message::Open(i)),
                        c,
                    )
                });
                side_by_side(
                    ui::panel(
                        Some(s.dvw_done),
                        column![
                            Column::with_children(rows).spacing(3),
                            container(ui::pill(s.ok, true, Some(Message::Close), c))
                                .width(Length::Fill)
                                .align_x(Alignment::End),
                        ]
                        .spacing(16),
                        c,
                    ),
                    summary,
                )
            }
            _ => self.form.view(s, c, Message::Form, Some(summary)),
        }
    }

    /// The file: its teams, the day, the season and the sets.
    fn summary<'a>(&self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let p = &self.preview;
        let days = match p.days.is_empty() {
            true => "?".to_string(),
            false => p.days.join(" / "),
        };
        let season = p.season.map_or("?".to_string(), |y| y.to_string());
        let sets = p
            .sets
            .iter()
            .map(|set| format!("{}–{}", set.home, set.visiting))
            .collect::<Vec<_>>()
            .join("  ");
        ui::panel(
            Some(&self.name),
            column![
                text(format!("{} – {}", p.home, p.visiting))
                    .size(SIZE_BODY)
                    .font(theme::fonts().bold),
                text(format!("{days} · {} {season}", s.dvw_season))
                    .size(SIZE_SMALL)
                    .color(c.muted),
                text(format!("{}: {sets}", ui::capitalized(s.dvw_sets)))
                    .size(SIZE_SMALL)
                    .color(c.muted),
            ]
            .spacing(6),
            c,
        )
    }
}

fn with_empty(names: impl Iterator<Item = String>) -> Vec<String> {
    std::iter::once(EMPTY.to_string()).chain(names).collect()
}

/// A similar team, and why.
fn similar_label(s: &Strings, team: &DvwSimilarTeam) -> String {
    let why = [
        team.same_name.then(|| s.dvw_same_name.to_string()),
        (team.same_players > 0).then(|| {
            ui::fill(
                s.dvw_same_players,
                &[team.same_players.to_string(), team.players.to_string()],
            )
        }),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(", ");
    match why.is_empty() {
        true => format!("{} {}", team.name, team.year),
        false => format!("{} {} ({why})", team.name, team.year),
    }
}

fn buttons<'a>(
    no: &str,
    on_no: Message,
    yes: &str,
    on_yes: Message,
    c: Colors,
) -> Element<'a, Message> {
    container(
        row![
            ui::pill(no, false, Some(on_no), c),
            ui::pill(yes, true, Some(on_yes), c),
        ]
        .spacing(10),
    )
    .width(Length::Fill)
    .align_x(Alignment::End)
    .into()
}

/// The step, the file beside it, as the forms with an aside.
fn side_by_side<'a>(
    main: Element<'a, Message>,
    aside: Element<'a, Message>,
) -> Element<'a, Message> {
    row![
        container(main).width(Length::FillPortion(3)),
        container(aside).width(Length::FillPortion(2)),
    ]
    .spacing(14)
    .into()
}

#[cfg(test)]
#[allow(clippy::unreachable)]
mod tests {
    use {
        super::*,
        crate::{
            strings::strings,
            transfer::{self, Export, Imported, Importing, Picked},
        },
        scout4all_core::shapes::enums::LanguageEnum,
    };

    /// Three sets won by the home team (`Home`), on 2015-01-25: the file
    /// of the tests of the TUI.
    const THREE_SETS: &[u8] = include_bytes!("../../src/tests/fixtures/three_sets.dvw");

    fn app(dir: &tempfile::TempDir) -> Arc<Scout4All> {
        Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap()
    }

    fn screen(core: &Arc<Scout4All>) -> DvwScreen {
        let s = strings(LanguageEnum::It);
        let bytes = Arc::new(THREE_SETS.to_vec());
        DvwScreen::new(core.clone(), s, "match.dvw".into(), bytes, None).unwrap()
    }

    /// The new team: its level, its gender and its country.
    fn new_team(dvw: &mut DvwScreen) {
        let s = strings(LanguageEnum::It);
        for m in [
            form::Message::Choose(CLASSIFICATION, 1),
            form::Message::Choose(GENDER, 1),
            form::Message::Input(SEARCH, "Ital".into()),
        ] {
            dvw.update(s, Message::Form(m));
        }
    }

    #[test]
    fn the_home_team_imported_as_a_new_one_then_its_match_skipped() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let mut dvw = screen(&core);
        // the home team; the day is the file's: on to the substitutions
        dvw.key(s, key!(enter));
        assert!(matches!(dvw.step, Step::Substitutions));
        assert!(dvw.substitutions > 0);
        // no team like it: a new one, its level and gender still to say
        dvw.key(s, key!(enter));
        assert!(matches!(dvw.step, Step::NewTeam(_)));
        dvw.key(s, key!(enter));
        assert!(matches!(dvw.step, Step::NewTeam(_)));
        assert!(dvw.form.error.is_some());
        new_team(&mut dvw);
        dvw.key(s, key!(enter));
        let Step::Ready(ready, _) = &dvw.step else {
            unreachable!("the import is checked: {:?}", dvw.form.error)
        };
        assert!(!ready.saved_already);
        dvw.key(s, key!(enter));
        assert!(matches!(dvw.step, Step::Done));
        let Outcome::Team(team_id) = dvw.key(s, key!(enter)) else {
            unreachable!("the team imported opens")
        };
        assert_eq!(core.matches(team_id.clone()).unwrap().len(), 1);

        // again: the team is found, the match is there already
        let mut dvw = screen(&core);
        dvw.key(s, key!(enter));
        dvw.key(s, key!(enter));
        assert!(matches!(dvw.step, Step::Team(_)));
        dvw.key(s, key!(enter));
        let Step::Ready(ready, _) = &dvw.step else {
            unreachable!("the import is checked")
        };
        assert!(ready.saved_already);
        // Esc leaves it as it is
        dvw.key(s, BACK_KEY);
        assert!(matches!(dvw.step, Step::Done));
        assert!(matches!(dvw.key(s, key!(enter)), Outcome::Stay));
        assert!(matches!(dvw.key(s, BACK_KEY), Outcome::Back));
        assert_eq!(core.matches(team_id).unwrap().len(), 1);
    }

    /// Asked from a team: the team offered first, even if not like the
    /// one of the file; asked again, the side like it chosen to start.
    #[test]
    fn imported_from_a_team_into_it() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let form = scout4all_core::team_form::TeamForm {
            name: "Cortona".into(),
            classification: Some(scout4all_core::shapes::enums::TeamClassificationEnum::Regional),
            gender: Some(scout4all_core::shapes::enums::GenderEnum::Women),
            country: CountryCode::known("IT"),
            year: "2014".into(),
        };
        let cortona = core.save_team(None, form).unwrap();
        let from = |core: &Arc<Scout4All>| {
            let bytes = Arc::new(THREE_SETS.to_vec());
            DvwScreen::new(
                core.clone(),
                s,
                "m.dvw".into(),
                bytes,
                Some(cortona.clone()),
            )
            .unwrap()
        };
        let mut dvw = from(&core);
        assert_eq!(dvw.form.index(0), 0, "no side like it: the home one");
        // the visiting team, into ours
        dvw.update(s, Message::Form(form::Message::Choose(0, 1)));
        dvw.key(s, key!(enter));
        dvw.key(s, key!(enter));
        let Step::Team(similar) = &dvw.step else {
            unreachable!("the team asked from is offered")
        };
        assert_eq!(similar.first().map(|t| t.id.clone()), Some(cortona.clone()));
        assert_eq!(similar_label(s, &similar[0]), "Cortona 2014");
        dvw.key(s, key!(enter));
        assert!(matches!(dvw.step, Step::Ready(..)));
        dvw.key(s, key!(enter));
        assert!(matches!(dvw.step, Step::Done));
        assert!(matches!(dvw.key(s, BACK_KEY), Outcome::Back));
        assert_eq!(core.matches(cortona.clone()).unwrap().len(), 1);
        // now with the players of the visiting team
        assert_eq!(from(&core).form.index(0), 1);
    }

    #[test]
    fn esc_goes_back_a_step_then_leaves() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let mut dvw = screen(&core);
        dvw.key(s, key!(enter));
        dvw.key(s, key!(enter));
        assert!(matches!(dvw.step, Step::NewTeam(_)));
        assert!(matches!(dvw.key(s, BACK_KEY), Outcome::Stay));
        assert!(matches!(dvw.step, Step::Substitutions));
        // a number of substitutions that isn't one: said, not on
        dvw.update(s, Message::Form(form::Message::Input(0, "x".into())));
        dvw.key(s, key!(enter));
        assert!(matches!(dvw.step, Step::Substitutions));
        assert!(dvw.form.error.is_some());
        assert!(matches!(dvw.key(s, BACK_KEY), Outcome::Stay));
        assert!(matches!(dvw.step, Step::Sides));
        assert!(matches!(dvw.key(s, BACK_KEY), Outcome::Back));
    }

    #[test]
    fn a_team_and_its_match_exported_then_imported_elsewhere() {
        let s = strings(LanguageEnum::It);
        let here = tempfile::tempdir().unwrap();
        let core = app(&here);
        let mut dvw = screen(&core);
        dvw.key(s, key!(enter));
        dvw.key(s, key!(enter));
        new_team(&mut dvw);
        dvw.key(s, key!(enter));
        dvw.key(s, key!(enter));
        let Outcome::Team(team_id) = dvw.key(s, key!(enter)) else {
            unreachable!("the team imported opens")
        };
        let scout4all_mobile::MatchListing::Match { id: match_id, .. } =
            core.matches(team_id.clone()).unwrap().remove(0)
        else {
            unreachable!("the match is readable")
        };
        let picked = |export: Export| {
            let (name, bytes) = transfer::archive(&core, &export).unwrap();
            Picked {
                name,
                bytes: Arc::new(bytes),
            }
        };
        let team = picked(Export::Team(team_id.clone()));
        let game = picked(Export::Match(team_id.clone(), match_id.clone()));
        // the report: a PDF named after the match
        let report = picked(Export::Report(team_id.clone(), match_id.clone()));
        assert_eq!(report.name, format!("{match_id}.pdf"));
        assert!(report.bytes.starts_with(b"%PDF"));

        // elsewhere: the team, then its match from the home
        let there = tempfile::tempdir().unwrap();
        let other = app(&there);
        let mut importing = Importing::new(&other, team, None).unwrap();
        assert!(matches!(importing.key(key!(down)), transfer::Outcome::Stay));
        assert!(matches!(
            importing.key(key!(enter)),
            transfer::Outcome::Import
        ));
        let Ok(Imported::Team(imported)) = importing.import(&other) else {
            unreachable!("the team is imported")
        };
        assert_eq!(imported, team_id);
        let importing = Importing::new(&other, game, None).unwrap();
        let Ok(Imported::Match(into, m)) = importing.import(&other) else {
            unreachable!("the match is imported")
        };
        assert_eq!((into, m), (team_id.clone(), match_id));
        assert_eq!(other.matches(team_id).unwrap().len(), 1);
        assert!(transfer::is_dvw("Partita.DVW"));
        assert!(!transfer::is_dvw("partita.zip"));
    }

    #[test]
    fn a_similar_team_says_why() {
        let s = crate::strings::strings(scout4all_core::shapes::enums::LanguageEnum::It);
        let team = |same_name, same_players| DvwSimilarTeam {
            id: "t".to_string(),
            name: "Cortona".to_string(),
            year: 2026,
            same_name,
            same_players,
            players: 12,
        };
        assert_eq!(
            similar_label(s, &team(true, 9)),
            "Cortona 2026 (stesso nome, 9 giocatori su 12 uguali)"
        );
        assert_eq!(
            similar_label(s, &team(true, 0)),
            "Cortona 2026 (stesso nome)"
        );
    }
}
