//! The training of the seasons before: the teams of the same name with an
//! earlier year, the latest first. Read only by construction: the seasons
//! on the left (↑ ↓), on the right their totals and their programs with
//! theirs. Nothing to change here: the plan of a season is changed on its
//! own team.

use {
    crate::{strings::Strings, theme::Colors, theme::SIZE_SMALL, ui},
    iced::{
        Element, Length,
        widget::{Column, Id, column, container, row, scrollable, space, text},
    },
    scout4all_core::training::cycles::Level,
    scout4all_keys::{KeyCombination, key},
    scout4all_mobile::{
        Scout4All, TeamListing,
        training::{CycleRecord, TotalsRecord, TrainingScope},
    },
    std::sync::Arc,
};

const LIST: &str = "training-history";

/// A season: its team, the totals of all its training and its programs.
struct Season {
    year: u16,
    totals: TotalsRecord,
    programs: Vec<(CycleRecord, TotalsRecord)>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Row(usize),
}

pub struct HistoryTab {
    core: Arc<Scout4All>,
    team_id: String,
    seasons: Vec<Season>,
    selected: usize,
    error: Option<String>,
}

impl HistoryTab {
    pub fn new(core: Arc<Scout4All>, team_id: String) -> HistoryTab {
        let mut tab = HistoryTab {
            core,
            team_id,
            seasons: vec![],
            selected: 0,
            error: None,
        };
        tab.reload();
        tab
    }

    pub fn scroll(&self) -> Option<(Id, f32)> {
        let last = self.seasons.len().saturating_sub(1).max(1);
        Some((Id::new(LIST), self.selected as f32 / last as f32))
    }

    pub fn reload(&mut self) {
        match self.read() {
            Ok(seasons) => {
                self.selected = self.selected.min(seasons.len().saturating_sub(1));
                self.seasons = seasons;
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn read(&self) -> Result<Vec<Season>, scout4all_mobile::MobileError> {
        let team = self.core.team(self.team_id.clone())?;
        let mut earlier: Vec<(u16, String)> = self
            .core
            .teams()?
            .into_iter()
            .filter_map(|listed| match listed {
                TeamListing::Team { id, name, year, .. }
                    if name == team.name && year < team.year =>
                {
                    Some((year, id))
                }
                _ => None,
            })
            .collect();
        earlier.sort_by(|a, b| b.cmp(a));
        earlier
            .into_iter()
            .map(|(year, id)| {
                let totals = self.core.training_totals(id.clone(), TrainingScope::All)?;
                let programs = self
                    .core
                    .cycles(id.clone(), Level::Program)?
                    .into_iter()
                    .map(|p| {
                        let totals = self.core.training_totals(
                            id.clone(),
                            TrainingScope::Cycle {
                                level: Level::Program,
                                id: p.id.clone(),
                            },
                        )?;
                        Ok((p, totals))
                    })
                    .collect::<Result<Vec<_>, scout4all_mobile::MobileError>>()?;
                Ok(Season {
                    year,
                    totals,
                    programs,
                })
            })
            .collect()
    }

    pub fn update(&mut self, message: Message) {
        match message {
            Message::Row(i) => self.selected = i,
        }
    }

    pub fn key(&mut self, ck: KeyCombination) {
        let count = self.seasons.len();
        match ck {
            k if k == key!(down) && count > 0 => self.selected = (self.selected + 1) % count,
            k if k == key!(up) && count > 0 => {
                self.selected = self.selected.checked_sub(1).unwrap_or(count - 1)
            }
            _ => {}
        }
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        vec![
            ("↑ ↓".to_string(), String::new()),
            ("⇥".to_string(), s.calendar.to_string()),
            ("Esc".to_string(), s.back.to_string()),
        ]
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        if let Some(e) = &self.error {
            return ui::body(e.clone()).color(c.danger).into();
        }
        if self.seasons.is_empty() {
            return text(s.no_earlier_seasons)
                .size(SIZE_SMALL)
                .color(c.muted)
                .into();
        }
        let rows: Vec<Element<'a, Message>> = self
            .seasons
            .iter()
            .enumerate()
            .map(|(i, season)| {
                ui::list_row(
                    ui::name(season.year.to_string()),
                    i == self.selected,
                    Some(Message::Row(i)),
                    c,
                )
            })
            .collect();
        let seasons = scrollable(Column::with_children(rows).spacing(3))
            .id(Id::new(LIST))
            .height(Length::Fill);
        let detail: Element<'a, Message> = match self.seasons.get(self.selected) {
            Some(season) => self.season_view(season, s, c),
            None => space::vertical().height(0).into(),
        };
        row![
            container(ui::panel(Some(s.season), seasons, c))
                .width(Length::FillPortion(2))
                .height(Length::Fill),
            container(detail)
                .width(Length::FillPortion(5))
                .height(Length::Fill),
        ]
        .spacing(14)
        .height(Length::Fill)
        .into()
    }

    fn season_view<'a>(
        &self,
        season: &Season,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let programs: Vec<Element<'a, Message>> = season
            .programs
            .iter()
            .map(|(p, totals)| {
                column![
                    ui::name(p.title.clone()),
                    text(p.window.clone()).size(SIZE_SMALL).color(c.muted),
                    text(summary(s, totals)).size(SIZE_SMALL),
                ]
                .spacing(2)
                .into()
            })
            .collect();
        let programs: Element<'a, Message> = match programs.is_empty() {
            true => text(s.no_cycles).size(SIZE_SMALL).color(c.muted).into(),
            false => scrollable(Column::with_children(programs).spacing(12))
                .height(Length::Fill)
                .into(),
        };
        ui::panel(
            Some(&season.year.to_string()),
            column![ui::body(summary(s, &season.totals)), programs].spacing(14),
            c,
        )
    }
}

/// "3 planned · 12 completed · 1 cancelled · 18 h 00 min".
fn summary(s: &Strings, t: &TotalsRecord) -> String {
    format!(
        "{} {} · {} {} · {} {} · {} {} h {:02} {} · {} {} h {:02} {}",
        t.planned,
        s.status_planned,
        t.completed,
        s.status_completed,
        t.cancelled,
        s.cancelled,
        s.planned_time,
        t.planned_minutes / 60,
        t.planned_minutes % 60,
        s.min_short,
        s.actual_time,
        t.actual_minutes / 60,
        t.actual_minutes % 60,
        s.min_short,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The earlier seasons of the same name, the latest first, with the
    /// totals; another name or a later year isn't there.
    #[test]
    fn the_earlier_seasons_of_the_team_are_read_with_their_totals() {
        let dir = tempfile::tempdir().unwrap();
        let core = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = |name: &str, year: u16| {
            let form = scout4all_core::team_form::TeamForm {
                name: name.to_string(),
                classification: Some(
                    scout4all_core::shapes::enums::TeamClassificationEnum::Regional,
                ),
                gender: Some(scout4all_core::shapes::enums::GenderEnum::Men),
                country: scout4all_core::shapes::country::CountryCode::known("IT"),
                ..core.new_team_form(year)
            };
            core.save_team(None, form).unwrap()
        };
        let (old, older, other, now, later) = (
            team("Vigili", 2025),
            team("Vigili", 2024),
            team("Altri", 2025),
            team("Vigili", 2026),
            team("Vigili", 2027),
        );
        let _ = (other, later);
        let draft = scout4all_mobile::training::SessionDraft {
            start: Some("2025-03-02T18:00".to_string()),
            duration: "90".into(),
            title: "Prima".into(),
            ..core.new_session_draft()
        };
        let id = core.save_session(old.clone(), None, draft).unwrap().id;
        core.complete_session(id, vec![]).unwrap();
        let mut tab = HistoryTab::new(core.clone(), now);
        assert!(tab.error.is_none());
        let years: Vec<u16> = tab.seasons.iter().map(|x| x.year).collect();
        assert_eq!(years, vec![2025, 2024]);
        assert_eq!(tab.seasons[0].totals.completed, 1);
        assert_eq!(tab.seasons[0].totals.actual_minutes, 90);
        assert_eq!(tab.seasons[1].totals.completed, 0);
        tab.key(key!(down));
        assert_eq!(tab.selected, 1);
        tab.key(key!(down));
        assert_eq!(tab.selected, 0);
        let _ = older;
    }
}
