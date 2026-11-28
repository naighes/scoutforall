//! The agenda of the training and the matches, as a panel: a week, only
//! the days with something planned and today, always there and lit (a
//! background of its own); "nothing planned" for a day without anything
//! and for a week without today. ↑ ↓ between the events, Enter opens one;
//! ‹ › (or PgUp PgDn) change the week, Home goes back to today.
//! The home shows every team (with its name), the team only its own.

use {
    crate::{
        calendar::{day_of, text_of},
        notice::{self, Level},
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL},
        ui,
    },
    chrono::{Duration, NaiveDate},
    iced::{
        Alignment, Background, Color, Element, Length, Padding,
        widget::{Column, column, container, row, scrollable, text},
    },
    scout4all_core::training::SessionStatus,
    scout4all_keys::{KeyCombination, key},
    scout4all_mobile::{
        MatchStanding, Scout4All,
        training::{AgendaDay, AgendaEventRecord, AgendaItemRecord, AgendaSpan},
    },
};

#[derive(Debug, Clone)]
pub enum Message {
    /// the row clicked
    Row(usize),
    Previous,
    Next,
    Today,
}

/// What a row picked asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Pick {
    /// a training session, to open
    Session { team_id: String, id: String },
    /// a match, to open
    Match { team_id: String, id: String },
}

/// A row: the event `.1` of the day `.0`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Slot(usize, usize);

pub struct Agenda {
    /// one team, or all
    team: Option<String>,
    /// the week of the home: without the teams hidden from it
    home: bool,
    /// a day of the period shown
    anchor: NaiveDate,
    today: NaiveDate,
    days: Vec<AgendaDay>,
    selected: usize,
    error: Option<String>,
}

impl Agenda {
    /// The week of `today`, of `team` (all the teams without).
    pub fn week(core: &Scout4All, team: Option<String>, today: NaiveDate) -> Agenda {
        Agenda::of(core, team, false, today)
    }

    /// The week of `today` as the home shows it.
    pub fn home_week(core: &Scout4All, today: NaiveDate) -> Agenda {
        Agenda::of(core, None, true, today)
    }

    fn of(core: &Scout4All, team: Option<String>, home: bool, today: NaiveDate) -> Agenda {
        let mut agenda = Agenda {
            team,
            home,
            anchor: today,
            today,
            days: vec![],
            selected: 0,
            error: None,
        };
        agenda.reload(core);
        agenda
    }

    /// Read again, after a change.
    pub fn reload(&mut self, core: &Scout4All) {
        let read = match self.home {
            true => core.home_week(text_of(self.anchor)),
            false => {
                core.training_agenda(self.team.clone(), AgendaSpan::Week, text_of(self.anchor))
            }
        };
        match read {
            Ok(days) => {
                self.days = days;
                self.error = None;
            }
            Err(e) => {
                self.days = vec![];
                self.error = Some(e.to_string());
            }
        }
        self.selected = self.selected.min(self.slots().len().saturating_sub(1));
    }

    /// Whether anything is planned in the period.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.days.iter().all(|d| d.events.is_empty())
    }

    fn slots(&self) -> Vec<Slot> {
        self.days
            .iter()
            .enumerate()
            .flat_map(|(d, day)| (0..day.events.len()).map(move |e| Slot(d, e)))
            .collect()
    }

    #[cfg(test)]
    pub fn slots_len(&self) -> usize {
        self.slots().len()
    }

    #[cfg(test)]
    pub fn pick(&self, index: usize) -> Option<Pick> {
        self.pick_of(index)
    }

    /// The days shown, each with whether it's today: the ones with
    /// something planned, and today.
    fn shown(&self) -> Vec<(&AgendaDay, bool)> {
        self.days
            .iter()
            .map(|d| (d, day_of(&d.date) == Some(self.today)))
            .filter(|(d, lit)| *lit || !d.events.is_empty())
            .collect()
    }

    #[cfg(test)]
    pub fn shown_days(&self) -> Vec<(String, bool)> {
        self.shown()
            .into_iter()
            .map(|(d, lit)| (d.date.clone(), lit))
            .collect()
    }

    fn pick_of(&self, index: usize) -> Option<Pick> {
        let Slot(d, e) = self.slots().get(index).copied()?;
        let event = self.days.get(d)?.events.get(e)?;
        Some(match &event.item {
            AgendaItemRecord::Session { id, .. } => Pick::Session {
                team_id: event.team_id.clone(),
                id: id.clone(),
            },
            AgendaItemRecord::Match { id, .. } => Pick::Match {
                team_id: event.team_id.clone(),
                id: id.clone(),
            },
        })
    }

    fn shift(&mut self, core: &Scout4All, weeks: i64) {
        self.anchor += Duration::weeks(weeks);
        self.selected = 0;
        self.reload(core);
    }

    fn go_today(&mut self, core: &Scout4All) {
        if self.anchor != self.today {
            self.anchor = self.today;
            self.selected = 0;
            self.reload(core);
        }
    }

    pub fn update(&mut self, core: &Scout4All, message: Message) -> Option<Pick> {
        match message {
            Message::Row(i) => {
                self.selected = i;
                self.pick_of(i)
            }
            Message::Previous => {
                self.shift(core, -1);
                None
            }
            Message::Next => {
                self.shift(core, 1);
                None
            }
            Message::Today => {
                self.go_today(core);
                None
            }
        }
    }

    pub fn key(&mut self, core: &Scout4All, ck: KeyCombination) -> Option<Pick> {
        let count = self.slots().len();
        match ck {
            k if k == key!(down) && count > 0 => self.selected = (self.selected + 1) % count,
            k if k == key!(up) && count > 0 => {
                self.selected = self.selected.checked_sub(1).unwrap_or(count - 1)
            }
            k if k == key!(pagedown) => self.shift(core, 1),
            k if k == key!(pageup) => self.shift(core, -1),
            k if k == key!(home) => self.go_today(core),
            k if k == key!(enter) => return self.pick_of(self.selected),
            _ => {}
        }
        None
    }

    /// The keys the panel answers to, for the hints.
    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        vec![
            ("↑ ↓".to_string(), String::new()),
            ("PgUp PgDn".to_string(), s.week.to_string()),
            ("Home".to_string(), s.today.to_string()),
        ]
    }

    /// The week as "3/11 – 9/11".
    fn range(&self, s: &Strings) -> String {
        let (first, last) = (
            self.days.first().and_then(|d| day_of(&d.date)),
            self.days.last().and_then(|d| day_of(&d.date)),
        );
        match (first, last) {
            (Some(a), Some(b)) if a != b => format!("{} – {}", s.short_date(a), s.short_date(b)),
            (Some(a), _) => s.short_date(a),
            _ => String::new(),
        }
    }

    pub fn view<'a>(
        &'a self,
        s: &'static Strings,
        c: Colors,
        active: bool,
        with_team: bool,
    ) -> Element<'a, Message> {
        let (actions, body) = self.parts(s, c, active, with_team);
        ui::panel_with(s.week, actions, body, c)
    }

    /// The actions of the week (its dates, today, the arrows) and its
    /// days, to put in a panel.
    pub fn parts<'a>(
        &'a self,
        s: &'static Strings,
        c: Colors,
        active: bool,
        with_team: bool,
    ) -> (Vec<Element<'a, Message>>, Element<'a, Message>) {
        let mut index = 0;
        let shown = self.shown();
        let nothing = || -> Element<'a, Message> {
            text(s.nothing_planned)
                .size(SIZE_SMALL)
                .color(c.muted)
                .into()
        };
        let mut rows: Vec<Element<'a, Message>> = vec![];
        // a week without today and without anything says it in a line
        if shown.is_empty() {
            rows.push(nothing());
        }
        for (day, lit) in shown {
            let mut lines: Vec<Element<'a, Message>> = vec![];
            if let Some(date) = day_of(&day.date) {
                lines.push(
                    text(format!("{} {}", s.weekday(date), s.short_date(date)))
                        .size(SIZE_SMALL)
                        .font(theme::fonts().bold)
                        .color(match lit {
                            true => c.accent,
                            false => c.muted,
                        })
                        .into(),
                );
            }
            if day.events.is_empty() {
                lines.push(nothing());
            }
            for event in &day.events {
                lines.push(event_row(
                    event,
                    s,
                    c,
                    with_team,
                    active && self.selected == index,
                    index,
                ));
                index += 1;
            }
            let group = Column::with_children(lines).spacing(3);
            rows.push(match lit {
                true => today_box(group, c),
                false => group.into(),
            });
        }
        let body: Element<'a, Message> = match &self.error {
            Some(e) => notice::banner(Level::Error, e, c),
            None => Column::with_children(rows).spacing(3).into(),
        };
        let mut actions: Vec<Element<'a, Message>> =
            vec![text(self.range(s)).size(SIZE_SMALL).color(c.muted).into()];
        actions.push(ui::today_pill(s.today, Message::Today, c));
        // the arrows stay side by side where the actions wrap
        let arrows = row![
            ui::tip(
                ui::pill("‹", false, Some(Message::Previous), c),
                s.previous_week,
                c,
            ),
            ui::tip(ui::pill("›", false, Some(Message::Next), c), s.next_week, c,),
        ]
        .spacing(8);
        actions.push(arrows.into());
        (actions, scrollable(body).height(Length::Fill).into())
    }
}

/// Today, lit: a background of the accent, light, under its rows.
pub fn today_box<'a, M: 'a>(content: impl Into<Element<'a, M>>, c: Colors) -> Element<'a, M> {
    container(content)
        .width(Length::Fill)
        .padding(Padding::from([4, 6]))
        .style(move |_| container::Style {
            background: Some(Background::Color(Color {
                a: 0.07,
                ..c.accent
            })),
            border: iced::border::rounded(6),
            ..container::Style::default()
        })
        .into()
}

/// What an event is called in a line: a match "Team – Opponent" with the
/// team, the opponent alone without; a session "Team · Title" or the title,
/// and the team alone when it has no title (`title` of a match is
/// "– Opponent").
pub fn heading(event: &AgendaEventRecord, title: String, with_team: bool) -> String {
    match (with_team, &event.item) {
        (true, AgendaItemRecord::Match { .. }) => format!("{} {title}", event.team_name),
        (_, AgendaItemRecord::Session { .. }) if title.is_empty() => event.team_name.clone(),
        (true, AgendaItemRecord::Session { .. }) => format!("{} · {title}", event.team_name),
        (false, AgendaItemRecord::Match { .. }) => title.trim_start_matches("– ").to_string(),
        (false, AgendaItemRecord::Session { .. }) => title,
    }
}

fn event_row<'a>(
    event: &AgendaEventRecord,
    s: &'static Strings,
    c: Colors,
    with_team: bool,
    chosen: bool,
    index: usize,
) -> Element<'a, Message> {
    let told = Told {
        with_team,
        times: None,
        unrecorded: false,
    };
    event_box(event, s, c, told, chosen, Message::Row(index))
}

/// What the box of an event tells besides the event itself.
pub struct Told {
    /// the name of its team (the events of all the teams)
    pub with_team: bool,
    /// when it starts and ends, where the end matters
    pub times: Option<String>,
    /// a completed session whose attendance isn't written
    pub unrecorded: bool,
}

/// An event of a list: the time at its head over the icon (a session and a
/// match apart by it and by its color), what and where, the score of a
/// match and the sign of a gym held by two teams at its tail.
pub fn event_box<'a, M: Clone + 'a>(
    event: &AgendaEventRecord,
    s: &'static Strings,
    c: Colors,
    told: Told,
    chosen: bool,
    message: M,
) -> Element<'a, M> {
    let bold = theme::fonts().bold;
    let time = event.start.get(11..16).unwrap_or_default().to_string();
    let (icon, color, title, flags, score, cancelled) = match &event.item {
        AgendaItemRecord::Session {
            title,
            status,
            to_close,
            ..
        } => {
            let cancelled = *status == SessionStatus::Cancelled;
            let flags = [
                cancelled.then_some(s.cancelled),
                to_close.then_some(s.to_close),
                told.unrecorded.then_some(s.attendance_not_recorded),
            ];
            (
                theme::icons::CALENDAR,
                c.accent,
                title.clone(),
                flags.into_iter().flatten().collect::<Vec<_>>(),
                None,
                cancelled,
            )
        }
        AgendaItemRecord::Match {
            opponent,
            ours,
            theirs,
            standing,
            ..
        } => (
            theme::icons::VOLLEYBALL,
            c.break_point,
            format!("– {opponent}"),
            vec![],
            // won or lost in its color, once over
            ours.zip(*theirs).map(|(o, t)| {
                (
                    format!("{o}–{t}"),
                    ui::score_color(o, t, *standing == MatchStanding::Over, c),
                )
            }),
            false,
        ),
    };
    let head = column![
        text(time).size(SIZE_SMALL).font(bold),
        ui::icon(icon, 16.0, color),
    ]
    .spacing(2)
    .align_x(Alignment::Center);
    // another team holds the gym then: the error color and an icon
    let shared = event.shared_venue && !cancelled;
    let detail = told
        .times
        .into_iter()
        .chain(
            event
                .venue
                .as_ref()
                .map(|v| crate::venue_choice::venue_label(s, &v.name, v.deleted)),
        )
        .chain(flags.into_iter().map(str::to_string))
        .collect::<Vec<_>>()
        .join(" · ");
    let what = Column::new()
        .push(
            text(heading(event, title, told.with_team))
                .size(SIZE_BODY)
                .font(bold)
                .color(match cancelled {
                    true => c.muted,
                    false => c.text,
                }),
        )
        // no empty line: the title stays in the middle
        .push((!detail.is_empty()).then(|| {
            text(detail).size(SIZE_SMALL).color(match shared {
                true => c.danger,
                false => c.muted,
            })
        }))
        .spacing(1)
        .width(Length::Fill);
    let body = row![what]
        .push(shared.then(|| {
            ui::tip(
                ui::icon(theme::icons::WARNING, 16.0, c.danger),
                s.venue_busy_mark,
                c,
            )
        }))
        .push(score.map(|(score, color)| {
            text(score)
                .size(crate::theme::SIZE_TITLE)
                .font(bold)
                .color(color)
        }))
        .spacing(8)
        .align_y(Alignment::Center);
    ui::box_row(head, body, chosen, message, c)
}

#[cfg(test)]
mod tests {
    use {
        super::*, crate::strings::strings, scout4all_core::shapes::enums::LanguageEnum,
        scout4all_mobile::training::SessionDraft, std::sync::Arc,
    };

    fn app(dir: &tempfile::TempDir) -> Arc<Scout4All> {
        Scout4All::for_tests(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap()
    }

    fn session(core: &Scout4All, team: &str, start: &str, title: &str) -> String {
        let draft = SessionDraft {
            start: Some(start.to_string()),
            duration: "90".into(),
            title: title.into(),
            ..core.new_session_draft()
        };
        core.save_session(team.to_string(), None, draft).unwrap().id
    }

    fn day(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
    }

    fn picked_session(pick: Option<Pick>) -> Option<String> {
        match pick {
            Some(Pick::Session { id, .. }) => Some(id),
            _ => None,
        }
    }

    /// Only the days with something and today are shown; ↑ from the first
    /// row goes round to the last, ↓ from the last to the first; a click on
    /// a row picks it.
    #[test]
    fn the_rows_go_round_and_a_click_picks_one() {
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        let monday = session(&core, &team, "2026-03-02T07:00", "Lunedì");
        let friday = session(&core, &team, "2026-03-06T18:00", "Venerdì");
        let mut agenda = Agenda::week(&core, Some(team.clone()), day("2026-03-04"));
        assert_eq!(
            agenda.shown_days(),
            vec![
                ("2026-03-02".to_string(), false),
                ("2026-03-04".to_string(), true),
                ("2026-03-06".to_string(), false),
            ]
        );
        assert_eq!(agenda.slots_len(), 2);
        agenda.key(&core, key!(up));
        assert_eq!(
            picked_session(agenda.key(&core, key!(enter))),
            Some(friday.clone())
        );
        agenda.key(&core, key!(down));
        assert_eq!(picked_session(agenda.key(&core, key!(enter))), Some(monday));
        assert_eq!(
            picked_session(agenda.update(&core, Message::Row(1))),
            Some(friday.clone())
        );
        // the row clicked is the one chosen
        assert_eq!(picked_session(agenda.key(&core, key!(enter))), Some(friday));
    }

    /// ‹ › change the week (the rows start again from the first), "Oggi"
    /// comes back; a week without today and without anything shows no day.
    #[test]
    fn the_arrows_change_the_week_and_today_comes_back() {
        let s = strings(LanguageEnum::It);
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        session(&core, &team, "2026-03-02T07:00", "Lunedì");
        session(&core, &team, "2026-03-10T18:00", "Martedì dopo");
        let mut agenda = Agenda::week(&core, Some(team), day("2026-03-04"));
        assert_eq!(
            agenda.range(s),
            format!(
                "{} – {}",
                s.short_date(day("2026-03-02")),
                s.short_date(day("2026-03-08"))
            )
        );
        agenda.update(&core, Message::Next);
        assert_eq!(agenda.shown_days(), vec![("2026-03-10".to_string(), false)]);
        agenda.update(&core, Message::Next);
        assert!(agenda.is_empty());
        assert!(agenda.shown_days().is_empty());
        assert_eq!(agenda.key(&core, key!(enter)), None);
        agenda.key(&core, key!(pageup));
        agenda.key(&core, key!(pageup));
        agenda.update(&core, Message::Previous);
        assert!(agenda.shown_days().is_empty());
        agenda.update(&core, Message::Today);
        assert_eq!(agenda.shown_days().len(), 2);
        assert_eq!(agenda.slots_len(), 1);
    }

    /// The home (no team) shows the sessions of every team.
    #[test]
    fn without_a_team_every_team_is_shown() {
        let dir = tempfile::tempdir().unwrap();
        let core = app(&dir);
        let team = core.tutorial_team().unwrap();
        let id = session(&core, &team, "2026-03-02T07:00", "Lunedì");
        let mut agenda = Agenda::week(&core, None, day("2026-03-04"));
        assert_eq!(
            agenda.update(&core, Message::Row(0)),
            Some(Pick::Session { team_id: team, id })
        );
    }

    fn event(item: AgendaItemRecord) -> AgendaEventRecord {
        AgendaEventRecord {
            team_id: "t".into(),
            team_name: "Volley".into(),
            start: "2026-03-02T18:00".into(),
            minutes: 90,
            item,
            venue: None,
            shared_venue: false,
        }
    }

    /// A match "Team – Opponent" with the team, the opponent alone without;
    /// a session "Team · Title" or the title; the team when it has none.
    #[test]
    fn the_heading_of_an_event() {
        let session = event(AgendaItemRecord::Session {
            id: "s".into(),
            title: String::new(),
            status: SessionStatus::Planned,
            to_close: false,
        });
        let game = event(AgendaItemRecord::Match {
            friendly: false,
            id: "m".into(),
            opponent: "Leoni".into(),
            opponent_team: None,
            standing: MatchStanding::ToPlay,
            ours: None,
            theirs: None,
        });
        assert_eq!(heading(&game, "– Leoni".into(), true), "Volley – Leoni");
        assert_eq!(heading(&game, "– Leoni".into(), false), "Leoni");
        assert_eq!(heading(&session, "Muro".into(), true), "Volley · Muro");
        assert_eq!(heading(&session, "Muro".into(), false), "Muro");
        assert_eq!(heading(&session, String::new(), true), "Volley");
        assert_eq!(heading(&session, String::new(), false), "Volley");
    }
}
