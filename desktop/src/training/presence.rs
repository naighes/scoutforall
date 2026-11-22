//! The attendance of the players over the completed sessions: the whole
//! season, a period, or a program, a mesocycle, a microcycle (← → change
//! it). Read only: the percentage counts present and absent, the late
//! ones are present, who was unavailable stays out of it, the guests are
//! not here.

use {
    super::screen::{Period, period_name},
    crate::{calendar::text_of, strings::Strings, theme::Colors, theme::SIZE_SMALL, ui},
    chrono::{Local, NaiveDate},
    iced::{
        Alignment, Element, Length,
        widget::{Column, Id, column, row, scrollable, text},
    },
    scout4all_core::training::cycles::Level,
    scout4all_keys::{KeyCombination, key},
    scout4all_mobile::{
        MobileError, Scout4All,
        training::{AttendanceReportRecord, TrainingScope},
    },
    std::sync::Arc,
};

const LIST: &str = "training-presence";

/// What the report counts, and how it reads.
#[derive(Debug, Clone, PartialEq)]
enum Scope {
    Season,
    Period(Period),
    Cycle {
        level: Level,
        id: String,
        title: String,
        /// the window, as saved
        first_day: String,
        last_day: String,
    },
}

#[derive(Debug, Clone)]
pub enum Message {
    Row(usize),
    Previous,
    Next,
}

pub struct PresenceTab {
    core: Arc<Scout4All>,
    team_id: String,
    today: NaiveDate,
    scopes: Vec<Scope>,
    scope: usize,
    report: AttendanceReportRecord,
    selected: usize,
    error: Option<String>,
}

impl PresenceTab {
    pub fn new(core: Arc<Scout4All>, team_id: String) -> PresenceTab {
        PresenceTab::at(core, team_id, Local::now().date_naive())
    }

    pub fn at(core: Arc<Scout4All>, team_id: String, today: NaiveDate) -> PresenceTab {
        let mut tab = PresenceTab {
            core,
            team_id,
            today,
            scopes: vec![],
            scope: 0,
            report: AttendanceReportRecord {
                sessions: 0,
                players: vec![],
            },
            selected: 0,
            error: None,
        };
        tab.reload();
        tab
    }

    pub fn scroll(&self) -> Option<(Id, f32)> {
        let last = self.report.players.len().saturating_sub(1).max(1);
        Some((Id::new(LIST), self.selected as f32 / last as f32))
    }

    /// The scopes there are now (the cycles change), the one chosen kept
    /// when it's still there, then the report.
    pub fn reload(&mut self) {
        let kept = self.scopes.get(self.scope).cloned();
        match self.read_scopes() {
            Ok(scopes) => {
                self.scope = kept
                    .and_then(|k| scopes.iter().position(|s| *s == k))
                    .unwrap_or(0);
                self.scopes = scopes;
                self.error = None;
            }
            Err(e) => {
                self.error = Some(e.to_string());
                return;
            }
        }
        self.read_report();
    }

    fn read_scopes(&self) -> Result<Vec<Scope>, MobileError> {
        let mut scopes = vec![
            Scope::Season,
            Scope::Period(Period::Week),
            Scope::Period(Period::Month),
            Scope::Period(Period::PreviousMonth),
        ];
        for level in [Level::Program, Level::Mesocycle, Level::Microcycle] {
            for cycle in self.core.cycles(self.team_id.clone(), level)? {
                scopes.push(Scope::Cycle {
                    level,
                    id: cycle.id,
                    title: cycle.title,
                    first_day: cycle.first_day,
                    last_day: cycle.last_day,
                });
            }
        }
        Ok(scopes)
    }

    fn training_scope(&self) -> TrainingScope {
        match self.scopes.get(self.scope) {
            Some(Scope::Period(period)) => match period.days(self.today) {
                Some((from, to)) => TrainingScope::Dates {
                    from: text_of(from),
                    to: text_of(to),
                },
                None => TrainingScope::All,
            },
            Some(Scope::Cycle { level, id, .. }) => TrainingScope::Cycle {
                level: *level,
                id: id.clone(),
            },
            _ => TrainingScope::All,
        }
    }

    fn read_report(&mut self) {
        match self
            .core
            .attendance_report(self.team_id.clone(), self.training_scope())
        {
            Ok(report) => {
                self.selected = self.selected.min(report.players.len().saturating_sub(1));
                self.report = report;
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn step(&mut self, forward: bool) {
        let count = self.scopes.len();
        if count == 0 {
            return;
        }
        self.scope = match forward {
            true => (self.scope + 1) % count,
            false => self.scope.checked_sub(1).unwrap_or(count - 1),
        };
        self.selected = 0;
        self.read_report();
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Row(i) => self.selected = i,
            Message::Previous => self.step(false),
            Message::Next => self.step(true),
        }
    }

    pub fn key(&mut self, ck: KeyCombination) {
        let count = self.report.players.len();
        match ck {
            k if k == key!(down) && count > 0 => self.selected = (self.selected + 1) % count,
            k if k == key!(up) && count > 0 => {
                self.selected = self.selected.checked_sub(1).unwrap_or(count - 1)
            }
            k if k == key!(right) => self.step(true),
            k if k == key!(left) => self.step(false),
            _ => {}
        }
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        vec![
            ("↑ ↓".to_string(), String::new()),
            ("← →".to_string(), s.period.to_string()),
            ("⇥".to_string(), ui::capitalized(s.calendar)),
            ("Esc".to_string(), s.back.to_string()),
        ]
    }

    fn scope_name(&self, s: &Strings) -> String {
        match self.scopes.get(self.scope) {
            Some(Scope::Period(period)) => period_name(s, *period).to_string(),
            Some(Scope::Cycle {
                title,
                first_day,
                last_day,
                ..
            }) => format!("{title} · {}", s.window(first_day, last_day)),
            _ => s.whole_season.to_string(),
        }
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        if let Some(e) = &self.error {
            return ui::body(e.clone()).color(c.danger).into();
        }
        let header = row![
            ui::pill("‹", false, Some(Message::Previous), c),
            ui::name(self.scope_name(s)),
            ui::pill("›", false, Some(Message::Next), c),
            text(format!(
                "{} {}",
                self.report.sessions,
                s.sessions.to_lowercase()
            ))
            .size(SIZE_SMALL)
            .color(c.muted),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let body: Element<'a, Message> = match self.report.players.is_empty() {
            true => text(s.no_attendance_to_count)
                .size(SIZE_SMALL)
                .color(c.muted)
                .into(),
            false => {
                let rows: Vec<Element<'a, Message>> = self
                    .report
                    .players
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let number = p.number.map(|n| n.to_string()).unwrap_or_default();
                        let base = p.present + p.absent;
                        let percent = match p.percent {
                            Some(v) => format!("{v}% {} {base}", s.on_sessions),
                            None => "–".to_string(),
                        };
                        let makeups = match p.makeups {
                            0 => String::new(),
                            n => format!(" · {}", ui::fill(s.makeups_count, &[n.to_string()])),
                        };
                        // the word, then the count (as on Android): "2 presente"
                        // isn't Italian
                        let late = match p.late {
                            0 => String::new(),
                            n => format!(" ({} {n})", s.late),
                        };
                        let counts = format!(
                            "{} {}{late} · {} {} · {} {}{makeups}",
                            s.present, p.present, s.absent, p.absent, s.unavailable, p.unavailable,
                        );
                        ui::list_row(
                            row![
                                text(number).width(Length::Fixed(36.0)),
                                ui::name(p.name.clone()),
                                text(counts).size(SIZE_SMALL).color(c.muted),
                                text(percent).width(Length::Fixed(130.0)),
                            ]
                            .spacing(12)
                            .align_y(Alignment::Center),
                            i == self.selected,
                            Some(Message::Row(i)),
                            c,
                        )
                    })
                    .collect();
                scrollable(Column::with_children(rows).spacing(3))
                    .id(Id::new(LIST))
                    .height(Length::Fill)
                    .into()
            }
        };
        ui::panel(
            Some(s.attendance),
            column![header, body].spacing(12).height(Length::Fill),
            c,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core() -> (tempfile::TempDir, Arc<Scout4All>) {
        let dir = tempfile::tempdir().unwrap();
        let core = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        (dir, core)
    }

    /// An empty team reads the whole season with nothing to count; ← →
    /// go around the scopes (season, three periods) and back.
    #[test]
    fn the_scopes_go_around_and_an_empty_report_is_empty() {
        let (_dir, core) = core();
        let form = scout4all_core::team_form::TeamForm {
            name: "Vigili".to_string(),
            classification: Some(scout4all_core::shapes::enums::TeamClassificationEnum::Regional),
            gender: Some(scout4all_core::shapes::enums::GenderEnum::Men),
            country: scout4all_core::shapes::country::CountryCode::known("IT"),
            ..core.new_team_form(2026)
        };
        let team = core.save_team(None, form).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
        let mut tab = PresenceTab::at(core, team, today);
        assert!(tab.error.is_none());
        assert_eq!(tab.scopes.len(), 4);
        assert!(tab.report.players.is_empty());
        tab.key(key!(left));
        assert_eq!(tab.scope, 3);
        tab.key(key!(right));
        assert_eq!(tab.scope, 0);
    }

    /// The players of a completed session with attendance written are
    /// counted over the whole season, and a period without it has none.
    #[test]
    fn the_report_counts_the_completed_sessions_with_attendance() {
        let (_dir, core) = core();
        let team = core.tutorial_team().unwrap();
        let draft = scout4all_mobile::training::SessionDraft {
            start: Some("2026-11-10T18:00".to_string()),
            duration: "90".into(),
            title: "Martedì".into(),
            ..core.new_session_draft()
        };
        let id = core.save_session(team.clone(), None, draft).unwrap().id;
        let rows = core.session_attendance(id.clone()).unwrap().rows;
        assert!(!rows.is_empty());
        core.save_attendance(id.clone(), rows.clone()).unwrap();
        core.complete_session(id, vec![]).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 11, 22).unwrap();
        let mut tab = PresenceTab::at(core, team, today);
        assert_eq!(tab.report.sessions, 1);
        assert_eq!(tab.report.players.len(), rows.len());
        assert_eq!(tab.report.players[0].percent, Some(100));
        // the previous month (October) has no session
        tab.update(Message::Next);
        tab.update(Message::Next);
        tab.update(Message::Next);
        assert_eq!(
            tab.scope_name(crate::strings::strings(
                scout4all_core::shapes::enums::LanguageEnum::En
            )),
            "last month"
        );
        assert_eq!(tab.report.sessions, 0);
    }
}
