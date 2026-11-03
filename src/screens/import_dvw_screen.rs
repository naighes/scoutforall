//! The import of a DataVolley file (`.dvw`), a step at a time: the teams to
//! import, the date when the file can't tell it, the substitutions, then
//! for each team ours (a similar one or a new one), the check and the save.
//! A team failing doesn't stop the other one.

use crate::localization::current_labels;
use crate::screens::{
    components::{
        country_picker::CountryPicker, date_picker::DatePicker,
        navigation_footer::NavigationFooter, notify_banner::NotifyBanner, panel, select::Select,
        text_box::TextBox,
    },
    screen::{
        AppAction, FieldKey, Renderable, Sba, ScreenAsync, change_value_entry,
        get_keybinding_actions, navigation_entries,
    },
};
use crate::shapes::keybinding::navigation_keybindings;
use crate::shapes::{actions::ScreenActionEnum, keybinding::ActionsKeyBindings};
use async_trait::async_trait;
use chrono::{Local, NaiveDate};
use crokey::crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{List, ListItem, ListState, Paragraph, Wrap},
};
use scout4all_core::{
    analytics::finished::GlobalFinishedMatches,
    constants::DEFAULT_MAX_SUBSTITUTIONS,
    dvw::{
        import::{
            MatchPlan, TeamChoice, TeamPlan, default_max_substitutions, match_date, match_exists,
            match_plan, save_import, season_year, similar_teams, team_plan,
        },
        model::{DvwFile, MatchDate, Side},
    },
    providers::{
        match_reader::MatchReader, match_writer::MatchWriter, set_writer::SetWriter,
        team_reader::TeamReader, team_writer::TeamWriter,
    },
    shapes::{country::CountryCode, enums::GenderEnum, team::TeamEntry},
};
use std::sync::Arc;

/// Where the import reads and saves, and the country of the new teams.
#[derive(Clone)]
pub struct DvwProviders {
    pub team_reader: Arc<dyn TeamReader + Send + Sync>,
    pub team_writer: Arc<dyn TeamWriter + Send + Sync>,
    pub match_reader: Arc<dyn MatchReader + Send + Sync>,
    pub match_writer: Arc<dyn MatchWriter + Send + Sync>,
    pub set_writer: Arc<dyn SetWriter + Send + Sync>,
    pub country: Option<CountryCode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Sides,
    Date,
    Substitutions,
    Team,
    NewTeam,
    Ready,
    Overwrite,
    Done,
}

pub struct DvwImportScreen {
    file: DvwFile,
    name: String,
    providers: DvwProviders,
    teams: Vec<TeamEntry>,
    step: Step,
    /// The choices of the step shown as a list.
    options: Vec<String>,
    list: ListState,
    sides: Vec<Side>,
    /// The side being imported, among `sides`.
    current: usize,
    day: Option<NaiveDate>,
    date: DatePicker,
    substitutions: TextBox,
    /// The similar teams of the side being imported.
    similar: Vec<TeamEntry>,
    gender: Select<GenderEnum>,
    country: CountryPicker,
    /// The field of the new team: gender or country.
    field: usize,
    plan: Option<(TeamPlan, MatchPlan)>,
    /// What happened to each side.
    results: Vec<String>,
    notify_message: NotifyBanner,
    footer: NavigationFooter,
    screen_key_bindings: ActionsKeyBindings<ScreenActionEnum>,
}

impl DvwImportScreen {
    /// `teams` are ours, for the similar ones.
    pub fn new(
        file: DvwFile,
        name: String,
        providers: DvwProviders,
        teams: Vec<TeamEntry>,
    ) -> Self {
        let labels = current_labels();
        let actions = &[
            Sba::Simple(ScreenActionEnum::Confirm),
            Sba::Simple(ScreenActionEnum::Back),
        ];
        let screen_key_bindings = navigation_keybindings().slice(Sba::keys(actions));
        let country = CountryPicker::new(labels.country.to_owned(), providers.country, false);
        let mut screen = Self {
            file,
            name,
            providers,
            teams,
            step: Step::Sides,
            options: vec![],
            list: ListState::default(),
            sides: vec![],
            current: 0,
            day: None,
            date: DatePicker::new(labels.date.to_owned(), true),
            substitutions: TextBox::with_validator(
                labels.max_substitutions.to_owned(),
                true,
                None,
                |current: &str, c: char| current.len() < 2 && c.is_ascii_digit(),
            ),
            similar: vec![],
            gender: Select::new(
                labels.gender.to_owned(),
                GenderEnum::ALL.to_vec(),
                None,
                true,
            ),
            country,
            field: 0,
            plan: None,
            results: vec![],
            notify_message: NotifyBanner::new(),
            footer: NavigationFooter::new(),
            screen_key_bindings,
        };
        screen.show_sides();
        screen
    }

    fn team_name(&self, side: Side) -> String {
        self.file.team(side).name.trim().to_string()
    }

    fn side_name(&self) -> String {
        self.sides
            .get(self.current)
            .map(|side| self.team_name(*side))
            .unwrap_or_default()
    }

    fn choose(&mut self, step: Step, options: Vec<String>) {
        self.step = step;
        self.options = options;
        self.list.select(Some(0));
    }

    fn show_sides(&mut self) {
        let labels = current_labels();
        let options = vec![
            labels
                .dvw_home
                .replace("{team}", &self.team_name(Side::Home)),
            labels
                .dvw_visiting
                .replace("{team}", &self.team_name(Side::Visiting)),
            labels.dvw_both.to_string(),
        ];
        self.choose(Step::Sides, options);
    }

    fn show_date(&mut self) {
        match self.file.match_info.date {
            MatchDate::Known(day) => {
                self.day = Some(day);
                self.show_substitutions();
            }
            MatchDate::Ambiguous(a, b) => {
                let options = [a, b].map(|d| d.format("%Y-%m-%d").to_string()).to_vec();
                self.choose(Step::Date, options);
            }
            MatchDate::Missing => {
                self.options = vec![];
                self.step = Step::Date;
            }
        }
    }

    /// The rules of the time of the match: six substitutions before 2026.
    fn show_substitutions(&mut self) {
        let max = default_max_substitutions(self.day);
        self.substitutions = TextBox::with_validator(
            current_labels().max_substitutions.to_owned(),
            true,
            Some(&max.to_string()),
            |current: &str, c: char| current.len() < 2 && c.is_ascii_digit(),
        );
        self.options = vec![];
        self.step = Step::Substitutions;
    }

    /// The team of the side being imported: a similar one, or a new one.
    fn show_team(&mut self) {
        let Some(side) = self.sides.get(self.current).copied() else {
            self.options = vec![];
            self.step = Step::Done;
            return;
        };
        let labels = current_labels();
        let found = similar_teams(&self.file, side, &self.teams);
        self.similar = found.iter().map(|s| s.team.clone()).collect();
        let options: Vec<String> = found
            .iter()
            .map(|s| {
                let why = [
                    s.same_name.then(|| labels.dvw_same_name.to_string()),
                    (s.same_players > 0).then(|| {
                        labels
                            .dvw_same_players
                            .replace("{same}", &s.same_players.to_string())
                            .replace("{players}", &s.players.to_string())
                    }),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(", ");
                format!("{} {} ({why})", s.team.name, s.team.year)
            })
            .chain([labels.dvw_new_team.to_string()])
            .collect();
        match found.is_empty() {
            true => self.show_new_team(),
            false => self.choose(Step::Team, options),
        }
    }

    fn show_new_team(&mut self) {
        let labels = current_labels();
        self.gender = Select::new(
            labels.gender.to_owned(),
            GenderEnum::ALL.to_vec(),
            None,
            true,
        );
        self.country = CountryPicker::new(labels.country.to_owned(), self.providers.country, false);
        self.field = 0;
        self.options = vec![];
        self.step = Step::NewTeam;
    }

    fn max_substitutions(&self) -> u8 {
        self.substitutions
            .get_selected_value()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_MAX_SUBSTITUTIONS)
    }

    /// The plan of the side with the team chosen, or why it can't be.
    fn plan(&mut self, choice: TeamChoice) {
        let labels = current_labels();
        let Some(side) = self.sides.get(self.current).copied() else {
            return;
        };
        let day = self.day.unwrap_or_default();
        let offset = *Local::now().offset();
        let planned = team_plan(&self.file, side, choice)
            .map_err(|e| e.describe(labels.domain))
            .and_then(|team| {
                match_plan(
                    &self.file,
                    side,
                    &team,
                    match_date(&self.file, day, offset),
                    self.max_substitutions(),
                )
                .map(|m| (team, m))
                .map_err(|e| e.describe(labels.domain))
            });
        match planned {
            Ok(plan) => {
                self.plan = Some(plan);
                self.options = vec![];
                self.step = Step::Ready;
            }
            Err(reason) => self.next_side(
                labels
                    .dvw_not_imported
                    .replace("{team}", &self.side_name())
                    .replace("{reason}", &reason),
            ),
        }
    }

    fn next_side(&mut self, result: String) {
        self.results.push(result);
        self.plan = None;
        self.current += 1;
        self.show_team();
    }

    async fn save(&mut self, overwrite: bool) {
        let labels = current_labels();
        let Some((team, m)) = self.plan.take() else {
            return;
        };
        let p = &self.providers;
        let saved = save_import(
            &team,
            &m,
            overwrite,
            &*p.team_writer,
            &*p.match_reader,
            &*p.match_writer,
            &*p.set_writer,
            &GlobalFinishedMatches,
        )
        .await;
        let result = match saved {
            Ok(_) => {
                self.teams.retain(|t| t.id != team.team.id);
                self.teams.push(team.team.clone());
                labels.dvw_imported.replace("{team}", &team.team.name)
            }
            Err(e) => labels
                .dvw_not_imported
                .replace("{team}", &team.team.name)
                .replace("{reason}", &e.to_string()),
        };
        self.next_side(result);
    }

    async fn confirm(&mut self) -> AppAction {
        let selected = self.list.selected().unwrap_or_default();
        match self.step {
            Step::Sides => {
                self.sides = match selected {
                    0 => vec![Side::Home],
                    1 => vec![Side::Visiting],
                    _ => vec![Side::Home, Side::Visiting],
                };
                self.show_date();
            }
            Step::Date => match self.file.match_info.date {
                MatchDate::Ambiguous(a, b) => {
                    self.day = Some(if selected == 0 { a } else { b });
                    self.show_substitutions();
                }
                _ => match self.date.get_selected_value() {
                    Ok(date) => {
                        self.day = Some(date.date_naive());
                        self.show_substitutions();
                    }
                    Err(e) => self.notify_message.set_error(e.to_string()),
                },
            },
            Step::Substitutions => match self.substitutions.get_selected_value() {
                Some(s) if s.parse::<u8>().is_ok_and(|n| n > 0) => {
                    self.current = 0;
                    self.show_team();
                }
                _ => self
                    .notify_message
                    .set_error(current_labels().max_substitutions.to_string()),
            },
            Step::Team => match self.similar.get(selected).cloned() {
                Some(team) => self.plan(TeamChoice::Existing(&team)),
                None => self.show_new_team(),
            },
            Step::NewTeam => {
                let choice = TeamChoice::New {
                    gender: self.gender.get_selected_value(),
                    country: self.country.get_selected_value(),
                };
                self.plan(choice);
            }
            Step::Ready => {
                let exists = match &self.plan {
                    Some((_, m)) => match_exists(m, &*self.providers.match_reader)
                        .await
                        .unwrap_or(false),
                    None => false,
                };
                match exists {
                    true => {
                        let labels = current_labels();
                        self.choose(
                            Step::Overwrite,
                            vec![
                                labels.dvw_overwrite.to_string(),
                                labels.dvw_dont_import.to_string(),
                            ],
                        );
                    }
                    false => self.save(false).await,
                }
            }
            Step::Overwrite => match selected {
                0 => self.save(true).await,
                _ => {
                    let result = current_labels()
                        .dvw_skipped
                        .replace("{team}", &self.side_name());
                    self.next_side(result);
                }
            },
            Step::Done => return AppAction::Back(true, Some(2)),
        }
        AppAction::None
    }

    /// Back a step; before the first one, back to the files.
    fn back(&mut self) -> AppAction {
        match self.step {
            Step::Sides => return AppAction::Back(false, Some(1)),
            Step::Date => self.show_sides(),
            Step::Substitutions => self.show_sides(),
            Step::Team => self.show_substitutions(),
            Step::NewTeam if !self.similar.is_empty() => self.show_team(),
            Step::NewTeam => self.show_substitutions(),
            // not saved: the side isn't imported
            Step::Ready | Step::Overwrite => {
                let result = current_labels()
                    .dvw_skipped
                    .replace("{team}", &self.side_name());
                self.next_side(result);
            }
            Step::Done => return AppAction::Back(true, Some(2)),
        }
        AppAction::None
    }

    fn move_selection(&mut self, forward: bool) {
        match self.step {
            Step::NewTeam => {
                self.field = (self.field + 1) % 2;
                self.gender.writing_mode = self.field == 0;
                self.country.writing_mode = self.field == 1;
            }
            _ if !self.options.is_empty() => {
                let count = self.options.len();
                let selected = self.list.selected().unwrap_or_default();
                let next = match forward {
                    true => (selected + 1).min(count - 1),
                    false => selected.saturating_sub(1),
                };
                self.list.select(Some(next));
            }
            _ => {}
        }
    }

    fn change(&mut self, back: bool) {
        match (self.step, back) {
            (Step::NewTeam, true) => {
                self.gender.handle_up();
                self.country.handle_up();
            }
            (Step::NewTeam, false) => {
                self.gender.handle_down();
                self.country.handle_down();
            }
            (_, back) => self.move_selection(!back),
        }
    }

    fn char(&mut self, c: char) {
        match self.step {
            Step::Date => self.date.handle_char(c),
            Step::Substitutions => self.substitutions.handle_char(c),
            Step::NewTeam => self.country.handle_char(c),
            _ => {}
        }
    }

    fn backspace(&mut self) {
        match self.step {
            Step::Date => self.date.handle_backspace(),
            Step::Substitutions => self.substitutions.handle_backspace(),
            Step::NewTeam => self.country.handle_backspace(),
            _ => {}
        }
    }

    /// The file: the teams, the day, the season and the sets.
    fn summary(&self) -> Vec<String> {
        let labels = current_labels();
        let day = match self.file.match_info.date {
            MatchDate::Known(d) => d.format("%Y-%m-%d").to_string(),
            MatchDate::Ambiguous(a, b) => {
                format!("{} / {}", a.format("%Y-%m-%d"), b.format("%Y-%m-%d"))
            }
            MatchDate::Missing => "?".to_string(),
        };
        let season = season_year(&self.file)
            .map(|y| y.to_string())
            .unwrap_or_else(|| "?".to_string());
        let sets = self
            .file
            .sets
            .iter()
            .map(|s| format!("{}-{}", s.final_score.0, s.final_score.1))
            .collect::<Vec<_>>()
            .join(", ");
        vec![
            format!(
                "{} - {}",
                self.team_name(Side::Home),
                self.team_name(Side::Visiting)
            ),
            format!("{day} · {} {season}", labels.dvw_season),
            format!("{}: {sets}", labels.dvw_sets),
        ]
    }

    /// What the step asks.
    fn question(&self) -> String {
        let labels = current_labels();
        let team = self.side_name();
        match self.step {
            Step::Sides => labels.dvw_which_team.to_string(),
            Step::Date => match self.file.match_info.date {
                MatchDate::Ambiguous(_, _) => labels.dvw_which_date.to_string(),
                _ => labels.dvw_no_date.to_string(),
            },
            Step::Substitutions => labels.dvw_substitutions.to_string(),
            Step::Team => labels.dvw_similar_found.replace("{team}", &team),
            Step::NewTeam => labels.dvw_new_team_of.replace("{team}", &team).replace(
                "{year}",
                &season_year(&self.file)
                    .map(|y| y.to_string())
                    .unwrap_or_default(),
            ),
            Step::Ready => match &self.plan {
                Some((t, m)) => labels
                    .dvw_ready
                    .replace("{team}", &t.team.name)
                    .replace("{sets}", &m.entry.sets.len().to_string())
                    .replace("{players}", &t.new_players.len().to_string()),
                None => String::new(),
            },
            Step::Overwrite => labels.dvw_already_saved.replace("{team}", &team),
            Step::Done => labels.dvw_done.to_string(),
        }
    }

    fn render_options(&mut self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .options
            .iter()
            .map(|o| ListItem::new(o.clone()))
            .collect();
        let list = List::new(items)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol(">> ");
        f.render_stateful_widget(list, area, &mut self.list);
    }
}

impl Renderable for DvwImportScreen {
    fn footer_entries(&self) -> Vec<(String, String)> {
        let actions = &[
            Sba::Simple(ScreenActionEnum::Confirm),
            Sba::Simple(ScreenActionEnum::Back),
        ];
        let change = match self.step {
            Step::NewTeam => vec![change_value_entry()],
            _ => vec![],
        };
        navigation_entries()
            .into_iter()
            .chain(change)
            .chain(get_keybinding_actions(navigation_keybindings(), actions))
            .collect()
    }

    fn render(&mut self, f: &mut Frame, body: Rect, footer_left: Rect, footer_right: Rect) {
        let labels = current_labels();
        let block = panel::screen(format!("{} - {}", labels.import_dvw, self.name));
        let inner = block.inner(body);
        f.render_widget(block, body);
        let summary = self.summary();
        let results = match self.step {
            Step::Done => self.results.clone(),
            _ => vec![],
        };
        let form_rows = |shown: bool| Constraint::Length(if shown { 3 } else { 1 });
        let area = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(summary.len() as u16),
                Constraint::Length(1),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(inner);
        f.render_widget(
            Paragraph::new(summary.into_iter().map(Line::from).collect::<Vec<_>>()),
            area[0],
        );
        f.render_widget(
            Paragraph::new(self.question())
                .style(Style::default().fg(Color::Cyan))
                .wrap(Wrap { trim: true }),
            area[2],
        );
        match self.step {
            Step::Date if self.options.is_empty() => self.date.render(f, area[3]),
            Step::Substitutions => self.substitutions.render(f, area[3]),
            Step::NewTeam => {
                let rows = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        form_rows(self.gender.writing_mode),
                        form_rows(self.country.writing_mode),
                        Constraint::Min(0),
                    ])
                    .split(area[3]);
                self.gender.render(f, rows[0]);
                self.country.render(f, rows[1]);
            }
            Step::Ready => {}
            Step::Done => f.render_widget(
                Paragraph::new(results.into_iter().map(Line::from).collect::<Vec<_>>())
                    .wrap(Wrap { trim: true }),
                area[3],
            ),
            _ => self.render_options(f, area[3]),
        }
        self.notify_message.render(f, footer_right);
        self.footer.render(f, footer_left, self.footer_entries());
    }
}

#[async_trait]
impl ScreenAsync for DvwImportScreen {
    async fn handle_key(&mut self, key: KeyEvent) -> AppAction {
        if self.notify_message.has_value() {
            self.notify_message.reset();
            return AppAction::None;
        }
        let action = self
            .screen_key_bindings
            .transform(key)
            .and_then(|k| self.screen_key_bindings.get(k));
        match (action, FieldKey::of(key.code), key.code) {
            (Some(ScreenActionEnum::Back), _, _) => self.back(),
            (Some(ScreenActionEnum::Confirm), _, _) => self.confirm().await,
            (_, Some(FieldKey::Next), _) => {
                self.move_selection(true);
                AppAction::None
            }
            (_, Some(FieldKey::Previous), _) => {
                self.move_selection(false);
                AppAction::None
            }
            (_, Some(FieldKey::Change { back }), _) => {
                self.change(back);
                AppAction::None
            }
            (_, _, KeyCode::Char(c)) => {
                self.char(c);
                AppAction::None
            }
            (_, _, KeyCode::Backspace) => {
                self.backspace();
                AppAction::None
            }
            _ => AppAction::None,
        }
    }

    async fn refresh_data(&mut self) {}
}
