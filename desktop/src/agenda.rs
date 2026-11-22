//! The agenda of the training and the matches, as a panel: a week (only
//! the days with something planned, today lit; "nothing planned" when no
//! day has) or a single day. ↑ ↓ between the events, Enter opens one;
//! ‹ › (or PgUp PgDn) change the week, Home goes back to today.
//! The home shows every team (with its name), the team only its own.

use {
    crate::{
        calendar::{day_of, text_of},
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL},
        ui,
    },
    chrono::{Duration, NaiveDate},
    iced::{
        Alignment, Element, Length,
        widget::{Column, column, row, scrollable, text},
    },
    scout4all_core::training::SessionStatus,
    scout4all_keys::{KeyCombination, key},
    scout4all_mobile::{
        Scout4All,
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
    span: AgendaSpan,
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
        Agenda::new(core, team, AgendaSpan::Week, today)
    }

    /// Today only.
    pub fn day(core: &Scout4All, team: Option<String>, today: NaiveDate) -> Agenda {
        Agenda::new(core, team, AgendaSpan::Day, today)
    }

    fn new(core: &Scout4All, team: Option<String>, span: AgendaSpan, today: NaiveDate) -> Agenda {
        let mut agenda = Agenda {
            team,
            span,
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
        match core.training_agenda(self.team.clone(), self.span, text_of(self.anchor)) {
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
        if self.span == AgendaSpan::Week {
            self.anchor += Duration::weeks(weeks);
            self.selected = 0;
            self.reload(core);
        }
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
        let mut hints = vec![("↑ ↓".to_string(), String::new())];
        if self.span == AgendaSpan::Week {
            hints.push(("PgUp PgDn".to_string(), s.week.to_string()));
            hints.push(("Home".to_string(), s.today.to_string()));
        }
        hints
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
        let mut rows: Vec<Element<'a, Message>> = vec![];
        let mut index = 0;
        // a day without anything isn't shown: the week says it only when
        // no day has anything
        if self.is_empty() {
            rows.push(
                text(s.nothing_planned)
                    .size(SIZE_SMALL)
                    .color(c.muted)
                    .into(),
            );
        }
        for day in self.days.iter().filter(|d| !d.events.is_empty()) {
            let date = day_of(&day.date);
            let lit = date == Some(self.today);
            if self.span == AgendaSpan::Week
                && let Some(date) = date
            {
                rows.push(
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
            for event in &day.events {
                rows.push(event_row(
                    event,
                    s,
                    c,
                    with_team,
                    active && self.selected == index,
                    index,
                ));
                index += 1;
            }
        }
        let body: Element<'a, Message> = match &self.error {
            Some(e) => ui::body(e.clone()).color(c.danger).into(),
            None => Column::with_children(rows).spacing(3).into(),
        };
        let mut actions: Vec<Element<'a, Message>> = vec![];
        if self.span == AgendaSpan::Week {
            actions.push(text(self.range(s)).size(SIZE_SMALL).color(c.muted).into());
            if self.anchor != self.today {
                actions.push(ui::pill(s.today, false, Some(Message::Today), c));
            }
            actions.push(ui::tip(
                ui::pill("‹", false, Some(Message::Previous), c),
                s.previous_week,
                c,
            ));
            actions.push(ui::tip(
                ui::pill("›", false, Some(Message::Next), c),
                s.next_week,
                c,
            ));
        }
        let title = match self.span {
            AgendaSpan::Day => s.today,
            _ => s.week,
        };
        let body: Element<'a, Message> = match self.span {
            AgendaSpan::Week => scrollable(body).height(Length::Fill).into(),
            _ => body,
        };
        ui::panel_with(title, actions, body, c)
    }
}

/// What an event is called in a line: a match "Team – Opponent" with the
/// team, the opponent alone without; a session "Team · Title" or the title
/// (`title` of a match is "– Opponent").
pub fn heading(event: &AgendaEventRecord, title: String, with_team: bool) -> String {
    match (with_team, &event.item) {
        (true, AgendaItemRecord::Match { .. }) => format!("{} {title}", event.team_name),
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
    let bold = theme::fonts().bold;
    let time = event.start.get(11..16).unwrap_or_default().to_string();
    let (icon, title, detail, trailing) = match &event.item {
        AgendaItemRecord::Session {
            title,
            status,
            to_close,
            ..
        } => {
            let mut flags = vec![];
            if *status == SessionStatus::Cancelled {
                flags.push(s.cancelled.to_string());
            }
            if *to_close {
                flags.push(s.to_close.to_string());
            }
            (
                theme::icons::CALENDAR,
                match title.is_empty() {
                    true => s.training.to_string(),
                    false => title.clone(),
                },
                [event
                    .venue
                    .as_ref()
                    .map(|v| crate::venue_choice::venue_label(s, &v.name, v.deleted))
                    .unwrap_or_default()]
                .into_iter()
                .chain(flags)
                .filter(|d| !d.is_empty())
                .collect::<Vec<_>>(),
                String::new(),
            )
        }
        AgendaItemRecord::Match {
            opponent,
            ours,
            theirs,
            ..
        } => (
            theme::icons::VOLLEYBALL,
            format!("– {opponent}"),
            vec![],
            match (ours, theirs) {
                (Some(o), Some(t)) => format!("{o}–{t}"),
                _ => String::new(),
            },
        ),
    };
    let heading = heading(event, title, with_team);
    let body = row![
        ui::icon(icon, 18.0, c.muted),
        column![
            text(heading).size(SIZE_BODY).font(bold),
            text(
                [time]
                    .into_iter()
                    .chain(detail)
                    .collect::<Vec<_>>()
                    .join(" · ")
            )
            .size(SIZE_SMALL)
            .color(c.muted),
        ]
        .spacing(1)
        .width(Length::Fill),
        text(trailing).size(SIZE_BODY).font(bold),
    ]
    .spacing(8)
    .align_y(Alignment::Center);
    ui::list_row(body, chosen, Some(Message::Row(index)), c)
}
