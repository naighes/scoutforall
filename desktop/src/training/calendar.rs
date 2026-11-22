//! The calendar of the training and the matches of a team: a week (seven
//! columns, the bands of its microcycles above) or a month (a cell a day,
//! the bands of its mesocycles above each row). Sessions and matches are
//! told apart by icon and color. ← → move the day, ↑ ↓ the event of the
//! day (the week) or the week (the month), PgUp PgDn change the period,
//! Home goes back to today, `w` and `m` choose the view, Enter opens the
//! event or, on a day without any, a new session (in the month it opens the
//! week of the day), `n` a new session, `r` a new series. No dragging.

use {
    crate::{
        calendar::{day_of, text_of},
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL},
        ui,
    },
    chrono::{Datelike, Days, Duration, Months, NaiveDate},
    iced::{
        Alignment, Background, Element, Length, Padding,
        widget::{Column, Row, column, container, row, scrollable, space, text},
    },
    scout4all_core::{training::SessionStatus, training::cycles::Level},
    scout4all_keys::{KeyCombination, key},
    scout4all_mobile::{
        Scout4All,
        training::{AgendaDay, AgendaEventRecord, AgendaItemRecord, AgendaSpan, CycleRecord},
    },
    std::sync::Arc,
};

/// The most events a month cell writes, the others are counted.
const CELL_EVENTS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum View {
    Week,
    Month,
}

#[derive(Debug, Clone)]
pub enum Message {
    Day(NaiveDate),
    Event(NaiveDate, usize),
    Previous,
    Next,
    Today,
    View(View),
    New,
    Repeat,
    /// this team or all the teams
    AllTeams,
    /// the next gym of the filter
    NextVenue,
}

/// What the calendar asks the screen for.
#[derive(Debug, Clone, PartialEq)]
pub enum Leave {
    Stay,
    Session(String),
    Match(String),
    NewSession(NaiveDate),
    Repeat(NaiveDate),
}

pub struct CalendarTab {
    core: Arc<Scout4All>,
    team_id: String,
    view: View,
    /// the day the cursor is on, and so the period shown
    anchor: NaiveDate,
    today: NaiveDate,
    /// the event of the anchor day the cursor is on
    slot: usize,
    days: Vec<AgendaDay>,
    /// the cycles whose bands are drawn: microcycles in the week,
    /// mesocycles in the month
    bands: Vec<CycleRecord>,
    /// the sessions and matches of every team, not only of this one
    all_teams: bool,
    /// the gym the events are limited to: id and name
    venue: Option<(String, String)>,
    error: Option<String>,
}

/// The Monday of the week of `day`.
pub fn monday(day: NaiveDate) -> NaiveDate {
    day - Duration::days(day.weekday().num_days_from_monday() as i64)
}

/// The weeks of the month of `day`, Monday first, `None` out of the month.
pub fn month_weeks(day: NaiveDate) -> Vec<[Option<NaiveDate>; 7]> {
    crate::calendar::weeks(day, false)
}

/// The first and the last day a view shows around `anchor`.
pub fn period(view: View, anchor: NaiveDate) -> (NaiveDate, NaiveDate) {
    match view {
        View::Week => {
            let first = monday(anchor);
            (first, first + Duration::days(6))
        }
        View::Month => {
            let first = crate::calendar::month_of(anchor);
            let last = first
                .checked_add_months(Months::new(1))
                .map(|next| next - Duration::days(1))
                .unwrap_or(first);
            (first, last)
        }
    }
}

/// The segments of a row of days a band covers: `(first index, days,
/// cycle)`, the days in a row under the same cycle together.
pub fn segments<'a>(
    days: &[Option<NaiveDate>],
    cycles: &'a [CycleRecord],
) -> Vec<(usize, usize, Option<&'a CycleRecord>)> {
    let cover = |day: &Option<NaiveDate>| {
        day.and_then(|d| {
            cycles.iter().find(|c| {
                day_of(&c.first_day).is_some_and(|f| f <= d)
                    && day_of(&c.last_day).is_some_and(|l| d <= l)
            })
        })
    };
    let mut out: Vec<(usize, usize, Option<&CycleRecord>)> = vec![];
    for (i, day) in days.iter().enumerate() {
        let now = cover(day);
        match out.last_mut() {
            Some((_, n, last)) if last.map(|c| &c.id) == now.map(|c| &c.id) => *n += 1,
            _ => out.push((i, 1, now)),
        }
    }
    out
}

impl CalendarTab {
    pub fn new(core: Arc<Scout4All>, team_id: String) -> CalendarTab {
        let today = chrono::Local::now().date_naive();
        let mut tab = CalendarTab {
            core,
            team_id,
            view: View::Week,
            anchor: today,
            today,
            slot: 0,
            days: vec![],
            bands: vec![],
            all_teams: false,
            venue: None,
            error: None,
        };
        tab.reload();
        tab
    }

    /// The tab on a given day (the tests'), instead of the real today.
    #[cfg(test)]
    pub fn at(core: Arc<Scout4All>, team_id: String, today: NaiveDate) -> CalendarTab {
        let mut tab = CalendarTab::new(core, team_id);
        tab.today = today;
        tab.anchor = today;
        tab.reload();
        tab
    }

    #[cfg(test)]
    pub fn view_shown(&self) -> View {
        self.view
    }

    #[cfg(test)]
    pub fn anchor(&self) -> NaiveDate {
        self.anchor
    }

    #[cfg(test)]
    pub fn days(&self) -> &[AgendaDay] {
        &self.days
    }

    #[cfg(test)]
    pub fn bands(&self) -> &[CycleRecord] {
        &self.bands
    }

    /// Read again, after a change or a move.
    pub fn reload(&mut self) {
        let span = match self.view {
            View::Week => AgendaSpan::Week,
            View::Month => AgendaSpan::Month,
        };
        let level = match self.view {
            View::Week => Level::Microcycle,
            View::Month => Level::Mesocycle,
        };
        let (first, last) = period(self.view, self.anchor);
        let read = self
            .core
            .training_agenda(
                (!self.all_teams).then(|| self.team_id.clone()),
                span,
                text_of(self.anchor),
            )
            .and_then(|mut days| {
                if let Some((venue, _)) = &self.venue {
                    for day in &mut days {
                        day.events
                            .retain(|e| e.venue.as_ref().is_some_and(|v| v.id == *venue));
                    }
                }
                let bands = self
                    .core
                    .cycles(self.team_id.clone(), level)?
                    .into_iter()
                    .filter(|c| {
                        day_of(&c.first_day).is_some_and(|f| f <= last)
                            && day_of(&c.last_day).is_some_and(|l| first <= l)
                    })
                    .collect();
                Ok((days, bands))
            });
        match read {
            Ok((days, bands)) => {
                self.days = days;
                self.bands = bands;
                self.error = None;
            }
            Err(e) => {
                self.days = vec![];
                self.bands = vec![];
                self.error = Some(e.to_string());
            }
        }
        self.slot = self.slot.min(self.events_of(self.anchor).saturating_sub(1));
    }

    fn day_record(&self, day: NaiveDate) -> Option<&AgendaDay> {
        self.days.iter().find(|d| day_of(&d.date) == Some(day))
    }

    fn events_of(&self, day: NaiveDate) -> usize {
        self.day_record(day).map_or(0, |d| d.events.len())
    }

    /// Move the cursor to `day`, reading again when the period changes.
    fn go_to(&mut self, day: NaiveDate) {
        let (first, last) = period(self.view, self.anchor);
        self.anchor = day;
        self.slot = 0;
        if day < first || day > last {
            self.reload();
        }
    }

    fn shift(&mut self, forward: bool) {
        let day = match (self.view, forward) {
            (View::Week, true) => self.anchor.checked_add_days(Days::new(7)),
            (View::Week, false) => self.anchor.checked_sub_days(Days::new(7)),
            (View::Month, true) => self.anchor.checked_add_months(Months::new(1)),
            (View::Month, false) => self.anchor.checked_sub_months(Months::new(1)),
        };
        if let Some(day) = day {
            self.anchor = day;
            self.slot = 0;
            self.reload();
        }
    }

    fn set_view(&mut self, view: View) {
        if self.view != view {
            self.view = view;
            self.slot = 0;
            self.reload();
        }
    }

    fn go_today(&mut self) {
        self.anchor = self.today;
        self.slot = 0;
        self.reload();
    }

    /// What Enter does on the cursor.
    fn open(&mut self) -> Leave {
        match self.view {
            View::Month => {
                self.set_view(View::Week);
                Leave::Stay
            }
            View::Week => self.pick(self.anchor, self.slot),
        }
    }

    /// The next gym of the filter: none, then each of them, then none.
    fn next_venue(&mut self) {
        let venues = self.core.venues().unwrap_or_default();
        self.venue = match &self.venue {
            None => venues.first(),
            Some((id, _)) => venues
                .iter()
                .position(|v| v.id == *id)
                .and_then(|at| venues.get(at + 1)),
        }
        .map(|v| (v.id.clone(), v.name.clone()));
        self.slot = 0;
        self.reload();
    }

    fn set_all_teams(&mut self) {
        self.all_teams = !self.all_teams;
        self.slot = 0;
        self.reload();
    }

    fn pick(&self, day: NaiveDate, slot: usize) -> Leave {
        // what another team holds opens from its own screen
        let event = self.day_record(day).and_then(|d| d.events.get(slot));
        if event.is_some_and(|e| e.team_id != self.team_id) {
            return Leave::Stay;
        }
        match event {
            Some(AgendaEventRecord {
                item: AgendaItemRecord::Session { id, .. },
                ..
            }) => Leave::Session(id.clone()),
            Some(AgendaEventRecord {
                item: AgendaItemRecord::Match { id, .. },
                ..
            }) => Leave::Match(id.clone()),
            None => Leave::NewSession(day),
        }
    }

    pub fn update(&mut self, message: Message) -> Leave {
        match message {
            Message::Day(day) => {
                self.go_to(day);
                Leave::Stay
            }
            Message::Event(day, slot) => {
                self.go_to(day);
                self.slot = slot;
                self.pick(day, slot)
            }
            Message::Previous => {
                self.shift(false);
                Leave::Stay
            }
            Message::Next => {
                self.shift(true);
                Leave::Stay
            }
            Message::Today => {
                self.go_today();
                Leave::Stay
            }
            Message::View(view) => {
                self.set_view(view);
                Leave::Stay
            }
            Message::New => Leave::NewSession(self.anchor),
            Message::Repeat => Leave::Repeat(self.anchor),
            Message::AllTeams => {
                self.set_all_teams();
                Leave::Stay
            }
            Message::NextVenue => {
                self.next_venue();
                Leave::Stay
            }
        }
    }

    pub fn key(&mut self, ck: KeyCombination) -> Leave {
        let by_days = |tab: &mut CalendarTab, days: i64| {
            let next = match days < 0 {
                true => tab.anchor.checked_sub_days(Days::new(days.unsigned_abs())),
                false => tab.anchor.checked_add_days(Days::new(days.unsigned_abs())),
            };
            if let Some(day) = next {
                tab.go_to(day);
            }
        };
        match ck {
            k if k == key!(left) => by_days(self, -1),
            k if k == key!(right) => by_days(self, 1),
            k if k == key!(up) && self.view == View::Month => by_days(self, -7),
            k if k == key!(down) && self.view == View::Month => by_days(self, 7),
            k if k == key!(up) => self.slot = self.slot.saturating_sub(1),
            k if k == key!(down) => {
                self.slot = (self.slot + 1).min(self.events_of(self.anchor).saturating_sub(1))
            }
            k if k == key!(pagedown) => self.shift(true),
            k if k == key!(pageup) => self.shift(false),
            k if k == key!(home) => self.go_today(),
            k if k == key!(w) => self.set_view(View::Week),
            k if k == key!(m) => self.set_view(View::Month),
            k if k == key!(enter) => return self.open(),
            k if k == key!(n) => return Leave::NewSession(self.anchor),
            k if k == key!(r) => return Leave::Repeat(self.anchor),
            k if k == key!(t) => self.set_all_teams(),
            k if k == key!(v) => self.next_venue(),
            _ => {}
        }
        Leave::Stay
    }

    pub fn hints(&self, s: &Strings) -> Vec<(String, String)> {
        let mut hints = vec![
            ("← →".to_string(), String::new()),
            (
                "↑ ↓".to_string(),
                match self.view {
                    View::Week => String::new(),
                    View::Month => s.week.to_string(),
                },
            ),
            ("PgUp PgDn".to_string(), String::new()),
            ("Home".to_string(), s.today.to_string()),
            (
                "w m".to_string(),
                format!("{} / {}", s.week, s.calendar_month),
            ),
            ("Enter".to_string(), s.open.to_string()),
            ("n".to_string(), s.new_session.to_string()),
            ("r".to_string(), s.repeat_sessions.to_string()),
            ("t".to_string(), s.all_teams.to_string()),
            ("v".to_string(), s.venues.to_string()),
        ];
        hints.push(("⇥".to_string(), s.sessions.to_string()));
        hints.push(("Esc".to_string(), s.back.to_string()));
        hints
    }

    /// What the cursor shows, for the scroll of the view: none, the
    /// calendar fits in its panel.
    fn title(&self, s: &Strings) -> String {
        let (first, last) = period(self.view, self.anchor);
        match self.view {
            View::Week => format!("{} – {}", s.short_date(first), s.short_date(last)),
            View::Month => format!(
                "{} {}",
                s.months
                    .get(first.month0() as usize)
                    .copied()
                    .unwrap_or_default(),
                first.year()
            ),
        }
    }

    pub fn view<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let (previous, next) = match self.view {
            View::Week => (s.previous_week, s.next_week),
            View::Month => (s.previous_month, s.next_month),
        };
        let mut controls: Vec<Element<'a, Message>> = vec![
            ui::pill(
                s.week,
                self.view == View::Week,
                Some(Message::View(View::Week)),
                c,
            ),
            ui::pill(
                s.calendar_month,
                self.view == View::Month,
                Some(Message::View(View::Month)),
                c,
            ),
            ui::tip(
                ui::pill("‹", false, Some(Message::Previous), c),
                previous,
                c,
            ),
            text(self.title(s))
                .size(SIZE_BODY)
                .font(theme::fonts().bold)
                .into(),
            ui::tip(ui::pill("›", false, Some(Message::Next), c), next, c),
        ];
        if period(self.view, self.anchor).0 > self.today
            || period(self.view, self.anchor).1 < self.today
        {
            controls.push(ui::pill(s.today, false, Some(Message::Today), c));
        }
        controls.push(space::horizontal().width(Length::Fill).into());
        controls.push(ui::pill(
            match self.all_teams {
                true => s.all_teams,
                false => s.this_team,
            },
            self.all_teams,
            Some(Message::AllTeams),
            c,
        ));
        controls.push(ui::pill(
            self.venue.as_ref().map_or(s.all_venues, |(_, name)| name),
            self.venue.is_some(),
            Some(Message::NextVenue),
            c,
        ));
        controls.push(ui::pill(s.new_session, false, Some(Message::New), c));
        controls.push(ui::pill(s.repeat_sessions, false, Some(Message::Repeat), c));
        let body: Element<'a, Message> = match &self.error {
            Some(e) => ui::body(e.clone()).color(c.danger).into(),
            None => match self.view {
                View::Week => self.week(s, c),
                View::Month => self.month(s, c),
            },
        };
        column![
            Row::with_children(controls)
                .spacing(6)
                .align_y(Alignment::Center),
            scrollable(body).height(Length::Fill)
        ]
        .spacing(10)
        .height(Length::Fill)
        .into()
    }

    /// The band of a row of days.
    fn band<'a>(
        &'a self,
        days: &[Option<NaiveDate>],
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let label = match self.view {
            View::Week => s.microcycle,
            View::Month => s.mesocycle,
        };
        Row::with_children(segments(days, &self.bands).into_iter().map(
            |(_, count, cycle)| -> Element<'a, Message> {
                let portion = Length::FillPortion(count as u16);
                match cycle {
                    None => space::horizontal().width(portion).into(),
                    Some(cycle) => container(
                        text(match cycle.title.is_empty() {
                            true => label.to_string(),
                            false => cycle.title.clone(),
                        })
                        .size(SIZE_SMALL)
                        .color(c.on_accent),
                    )
                    .width(portion)
                    .padding(Padding::from([1, 8]))
                    .style(move |_| container::Style {
                        background: Some(Background::Color(c.accent)),
                        border: iced::border::rounded(6),
                        ..container::Style::default()
                    })
                    .into(),
                }
            },
        ))
        .spacing(2)
        .into()
    }

    fn week<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let days: Vec<Option<NaiveDate>> = self
            .days
            .iter()
            .map(|d| day_of(&d.date))
            .collect::<Vec<_>>();
        let columns = Row::with_children(self.days.iter().map(|day| {
            let date = day_of(&day.date);
            let lit = date == Some(self.today);
            let here = date == Some(self.anchor);
            let head: Element<'a, Message> = match date {
                Some(date) => ui::list_row(
                    text(format!("{} {}", s.weekday(date), s.short_date(date)))
                        .size(SIZE_SMALL)
                        .font(theme::fonts().bold)
                        .color(match lit {
                            true => c.accent,
                            false => c.muted,
                        }),
                    here && day.events.is_empty(),
                    Some(Message::Day(date)),
                    c,
                ),
                None => space::vertical().into(),
            };
            let events = Column::with_children(day.events.iter().enumerate().map(|(i, event)| {
                ui::list_row(
                    event_text(event, s, c, false, self.all_teams),
                    here && i == self.slot,
                    date.map(|d| Message::Event(d, i)),
                    c,
                )
            }))
            .spacing(3);
            column![head, events]
                .spacing(3)
                .width(Length::FillPortion(1))
                .into()
        }))
        .spacing(4);
        column![self.band(&days, s, c), columns].spacing(6).into()
    }

    fn month<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let names = Row::with_children(s.weekdays.iter().map(|n| {
            text(n.to_string())
                .size(SIZE_SMALL)
                .color(c.muted)
                .width(Length::FillPortion(1))
                .align_x(Alignment::Center)
                .into()
        }))
        .spacing(4);
        let weeks = Column::with_children(month_weeks(self.anchor).into_iter().map(|week| {
            let cells = Row::with_children(week.iter().map(|day| -> Element<'a, Message> {
                match day {
                    None => space::horizontal().width(Length::FillPortion(1)).into(),
                    Some(day) => container(self.cell(*day, s, c))
                        .width(Length::FillPortion(1))
                        .into(),
                }
            }))
            .spacing(4);
            column![self.band(&week, s, c), cells].spacing(2).into()
        }))
        .spacing(8);
        column![names, weeks].spacing(6).into()
    }

    /// A day of the month: the number and the first events.
    fn cell<'a>(&'a self, day: NaiveDate, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let events: &[AgendaEventRecord] = self
            .day_record(day)
            .map(|d| d.events.as_slice())
            .unwrap_or_default();
        let mut lines: Vec<Element<'a, Message>> = vec![
            text(day.day().to_string())
                .size(SIZE_SMALL)
                .font(theme::fonts().bold)
                .color(match day == self.today {
                    true => c.accent,
                    false => c.muted,
                })
                .into(),
        ];
        lines.extend(
            events
                .iter()
                .take(CELL_EVENTS)
                .map(|e| event_text(e, s, c, true, self.all_teams)),
        );
        if events.len() > CELL_EVENTS {
            lines.push(
                text(format!("+{}", events.len() - CELL_EVENTS))
                    .size(SIZE_SMALL)
                    .color(c.muted)
                    .into(),
            );
        }
        ui::list_row(
            Column::with_children(lines).spacing(1),
            day == self.anchor,
            Some(Message::Day(day)),
            c,
        )
    }
}

/// An event as a line: the icon (a session and a match apart by it and by
/// its color), the time, the title; the week adds the place and the score.
fn event_text<'a>(
    event: &AgendaEventRecord,
    s: &'static Strings,
    c: Colors,
    compact: bool,
    with_team: bool,
) -> Element<'a, Message> {
    let time = event.start.get(11..16).unwrap_or_default().to_string();
    let (icon, color, title, detail) = match &event.item {
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
                c.accent,
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
            )
        }
        AgendaItemRecord::Match {
            opponent,
            ours,
            theirs,
            ..
        } => (
            theme::icons::VOLLEYBALL,
            c.break_point,
            format!("– {opponent}"),
            match (ours, theirs) {
                (Some(o), Some(t)) => vec![format!("{o}–{t}")],
                _ => vec![],
            },
        ),
    };
    let (cancelled, to_close) = match &event.item {
        AgendaItemRecord::Session {
            status, to_close, ..
        } => (*status == SessionStatus::Cancelled, *to_close),
        AgendaItemRecord::Match { .. } => (false, false),
    };
    let marks = match (compact, cancelled, to_close) {
        (false, ..) => String::new(),
        (true, true, true) => " × !".to_string(),
        (true, true, false) => " ×".to_string(),
        (true, false, true) => " !".to_string(),
        (true, false, false) => String::new(),
    };
    let title = crate::agenda::heading(event, title, with_team);
    // another team holds the gym then: the error color and an icon
    let shared = event.shared_venue && !cancelled;
    let first = text(format!("{time} {title}{marks}"))
        .size(SIZE_SMALL)
        .font(theme::fonts().bold)
        .color(match (cancelled, shared) {
            (true, _) => c.muted,
            (false, true) => c.danger,
            (false, false) => c.text,
        });
    let lines: Element<'a, Message> = match compact {
        true => first.into(),
        false => column![
            first,
            text(detail.join(" · ")).size(SIZE_SMALL).color(c.muted)
        ]
        .spacing(1)
        .into(),
    };
    let mut line = row![ui::icon(icon, 14.0, color), lines]
        .spacing(5)
        .align_y(Alignment::Center);
    if shared {
        line = line.push(ui::tip(
            ui::icon(theme::icons::WARNING, 14.0, c.danger),
            s.venue_busy_mark,
            c,
        ));
    }
    line.into()
}
