//! The calendar of the training and the matches of a team, as the
//! calendars of the mail draw it: a week (seven columns under the number of
//! their day, the hours on the left, each event as tall as it lasts, the
//! bands of its microcycles above) or a month (a box a day, the days of the
//! months around in grey, the bands of its mesocycles above each row). Sessions and matches are
//! told apart by icon and color. ← → move the day, ↑ ↓ the event of the
//! day (the week) or the week (the month), PgUp PgDn change the period,
//! Home goes back to today, `w` and `m` choose the view, Enter opens the
//! event or, on a day without any, a new session (in the month it opens the
//! week of the day), `n` a new session, `r` a new series. No dragging.

use {
    crate::{
        calendar::{day_of, text_of},
        strings::Strings,
        theme::{self, Colors, SIZE_BODY, SIZE_SMALL, SIZE_TITLE},
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
    /// the gyms there are: the filter shows only with some
    venues: Vec<(String, String)>,
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
            venues: vec![],
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
        self.venues = self
            .core
            .venues()
            .unwrap_or_default()
            .into_iter()
            .map(|v| (v.id, v.name))
            .collect();
        // a gym removed meanwhile filters no more
        if self
            .venue
            .as_ref()
            .is_some_and(|(id, _)| !self.venues.iter().any(|(v, _)| v == id))
        {
            self.venue = None;
        }
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
        let venues = &self.venues;
        self.venue = match &self.venue {
            None => venues.first(),
            Some((id, _)) => venues
                .iter()
                .position(|(v, _)| v == id)
                .and_then(|at| venues.get(at + 1)),
        }
        .cloned();
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
        ];
        if !self.venues.is_empty() {
            hints.push(("v".to_string(), s.venues.to_string()));
        }
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
        // without gyms there is nothing to filter by
        if !self.venues.is_empty() {
            controls.push(ui::pill(
                self.venue.as_ref().map_or(s.all_venues, |(_, name)| name),
                self.venue.is_some(),
                Some(Message::NextVenue),
                c,
            ));
        }
        controls.push(ui::pill(s.new_session, false, Some(Message::New), c));
        controls.push(ui::pill(s.repeat_sessions, false, Some(Message::Repeat), c));
        let body: Element<'a, Message> = match &self.error {
            Some(e) => ui::body(e.clone()).color(c.danger).into(),
            None => match self.view {
                View::Week => self.week(s, c),
                View::Month => self.month(s, c),
            },
        };
        // the week scrolls its hours under the heads of the days; the
        // month as a whole
        let body: Element<'a, Message> = match self.view {
            View::Week => body,
            View::Month => scrollable(body).height(Length::Fill).into(),
        };
        column![
            Row::with_children(controls)
                .spacing(6)
                .align_y(Alignment::Center),
            body
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

    /// The week as the calendars of the mail draw it: the days side by
    /// side under their number, the hours on the left, each event as tall
    /// as it lasts, the ones at the same time side by side.
    fn week<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let dates: Vec<Option<NaiveDate>> = self.days.iter().map(|d| day_of(&d.date)).collect();
        let (first, last) = hours(&self.days);
        let gutter = || space::horizontal().width(Length::Fixed(GUTTER));
        let heads = Row::with_children(
            std::iter::once(gutter().into())
                .chain(dates.iter().map(|date| self.day_head(*date, s, c))),
        )
        .spacing(1);
        let band = row![gutter(), self.band(&dates, s, c)];
        let labels = Column::with_children((first..last).map(|hour| {
            container(text(format!("{hour}:00")).size(SIZE_SMALL).color(c.muted))
                .width(Length::Fixed(GUTTER))
                .height(Length::Fixed(HOUR))
                .padding(Padding::ZERO.right(8))
                .align_x(Alignment::End)
                .into()
        }));
        // the lines between the hours and the days: the gaps between the
        // cells, over the color of the borders
        let columns = container(
            Row::with_children(
                self.days
                    .iter()
                    .map(|day| self.day_column(day, first, last, s, c)),
            )
            .spacing(1),
        )
        .padding(1)
        .style(move |_| container::Style {
            background: Some(Background::Color(c.border)),
            ..container::Style::default()
        });
        column![
            heads,
            band,
            scrollable(row![labels, columns]).height(Length::Fill)
        ]
        .spacing(6)
        .height(Length::Fill)
        .into()
    }

    /// The head of a day of the week: its number, large, and its name;
    /// today in the accent with a bar above, the day of the cursor lit.
    fn day_head<'a>(
        &'a self,
        date: Option<NaiveDate>,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let Some(date) = date else {
            return space::horizontal().width(Length::FillPortion(1)).into();
        };
        let lit = date == self.today;
        let color = match lit {
            true => c.accent,
            false => c.text,
        };
        let bar = container(space::vertical())
            .width(Length::Fill)
            .height(Length::Fixed(3.0))
            .style(move |_| container::Style {
                background: lit.then_some(Background::Color(c.accent)),
                ..container::Style::default()
            });
        let head = column![
            text(date.day().to_string())
                .size(SIZE_TITLE + 6.0)
                .font(theme::fonts().bold)
                .color(color),
            text(s.weekday(date)).size(SIZE_SMALL).color(match lit {
                true => c.accent,
                false => c.muted,
            }),
        ]
        .spacing(0);
        container(column![
            bar,
            ui::list_row(head, date == self.anchor, Some(Message::Day(date)), c)
        ])
        .width(Length::FillPortion(1))
        .into()
    }

    /// A day of the week: an hour a cell (a click chooses the day), today
    /// on a background of its own; over them the events, where their hours
    /// are.
    fn day_column<'a>(
        &'a self,
        day: &'a AgendaDay,
        first: u32,
        last: u32,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let date = day_of(&day.date);
        let background = match date == Some(self.today) {
            true => mix(c.panel, c.accent, 0.07),
            false => c.panel,
        };
        let grid = Column::with_children((first..last).map(|_| {
            iced::widget::button(space::vertical())
                .width(Length::Fill)
                .height(Length::Fixed(HOUR - 1.0))
                .on_press_maybe(date.map(Message::Day))
                .style(move |_, _| iced::widget::button::Style {
                    background: Some(Background::Color(background)),
                    ..iced::widget::button::Style::default()
                })
                .into()
        }))
        .spacing(1);
        let here = date == Some(self.anchor);
        let placed = lanes(&day.events);
        let layers = day.events.iter().zip(placed).enumerate().map(
            |(i, (event, (lane, count)))| -> Element<'a, Message> {
                let start = minute_of(&event.start).unwrap_or(first * 60);
                let top = (start.saturating_sub(first * 60) as f32 / 60.0 * HOUR).max(0.0);
                let tall = (f32::from(event.minutes) / 60.0 * HOUR).max(MIN_EVENT);
                let card = event_card(
                    event,
                    s,
                    c,
                    tall < TWO_LINES,
                    self.all_teams,
                    here && i == self.slot,
                    date.map(|d| Message::Event(d, i)),
                    tall,
                );
                let portion = |n: usize| Length::FillPortion(u16::try_from(n).unwrap_or(1));
                let mut parts: Vec<Element<'a, Message>> = vec![];
                if lane > 0 {
                    parts.push(space::horizontal().width(portion(lane)).into());
                }
                parts.push(
                    column![space::vertical().height(Length::Fixed(top)), card]
                        .width(portion(1))
                        .into(),
                );
                if count > lane + 1 {
                    parts.push(space::horizontal().width(portion(count - lane - 1)).into());
                }
                Row::with_children(parts)
                    .spacing(2)
                    .padding(Padding::from([0, 2]))
                    .into()
            },
        );
        iced::widget::Stack::with_children(std::iter::once(grid.into()).chain(layers))
            .width(Length::FillPortion(1))
            .into()
    }

    /// The month as the calendars of the mail draw it: a box a day, its
    /// number at the top (today in a circle of the accent), its first
    /// events below, the days of the months around in grey.
    fn month<'a>(&'a self, s: &'static Strings, c: Colors) -> Element<'a, Message> {
        let names = Row::with_children(s.weekdays.iter().map(|n| {
            text(n.to_string())
                .size(SIZE_SMALL)
                .color(c.muted)
                .width(Length::FillPortion(1))
                .into()
        }))
        .spacing(1)
        .padding(Padding::from([0, 8]));
        let weeks = Column::with_children(month_weeks(self.anchor).into_iter().map(|week| {
            let cells = Row::with_children(
                whole_week(&week)
                    .into_iter()
                    .map(|day| self.cell(day, week.contains(&Some(day)), s, c)),
            )
            .spacing(1);
            column![self.band(&week, s, c), cells].spacing(2).into()
        }))
        .spacing(1);
        column![names, weeks].spacing(6).into()
    }

    /// A day of the month: the number and the first events, the others
    /// counted; a day of the months around only its number, in grey.
    fn cell<'a>(
        &'a self,
        day: NaiveDate,
        in_month: bool,
        s: &'static Strings,
        c: Colors,
    ) -> Element<'a, Message> {
        let events: &[AgendaEventRecord] = match in_month {
            true => self
                .day_record(day)
                .map(|d| d.events.as_slice())
                .unwrap_or_default(),
            false => &[],
        };
        let number = text(match (day.day(), in_month) {
            (1, _) | (_, false) => format!(
                "{} {}",
                day.day(),
                s.months
                    .get(day.month0() as usize)
                    .and_then(|m| m.get(..3).or(Some(m)))
                    .unwrap_or_default()
            ),
            (d, true) => d.to_string(),
        })
        .size(SIZE_SMALL)
        .font(theme::fonts().bold);
        let number: Element<'a, Message> = match (day == self.today, in_month) {
            (true, _) => container(number.color(c.on_accent))
                .padding(Padding::from([1, 7]))
                .style(move |_| container::Style {
                    background: Some(Background::Color(c.accent)),
                    border: iced::border::rounded(10),
                    ..container::Style::default()
                })
                .into(),
            (false, true) => number.color(c.text).into(),
            (false, false) => number.color(c.muted).into(),
        };
        let mut lines: Vec<Element<'a, Message>> = vec![number];
        lines.extend(events.iter().take(CELL_EVENTS).enumerate().map(|(i, e)| {
            event_card(
                e,
                s,
                c,
                true,
                self.all_teams,
                false,
                Some(Message::Event(day, i)),
                MONTH_EVENT,
            )
        }));
        if events.len() > CELL_EVENTS {
            lines.push(
                text(format!("+{}", events.len() - CELL_EVENTS))
                    .size(SIZE_SMALL)
                    .color(c.muted)
                    .into(),
            );
        }
        let chosen = day == self.anchor;
        let background = match in_month {
            true => c.panel,
            false => c.background,
        };
        iced::widget::button(Column::with_children(lines).spacing(2))
            .width(Length::FillPortion(1))
            .height(Length::Fixed(CELL_HEIGHT))
            .padding(Padding::from([4, 6]))
            .on_press(Message::Day(day))
            .style(move |_, _| iced::widget::button::Style {
                background: Some(Background::Color(background)),
                text_color: c.text,
                border: match chosen {
                    true => iced::border::color(c.accent).width(2),
                    false => iced::border::color(c.border).width(1),
                },
                ..iced::widget::button::Style::default()
            })
            .into()
    }
}

/// The height of an hour in the week.
const HOUR: f32 = 44.0;

/// The hours the week shows at least: from 7 to 22.
const FIRST_HOUR: u32 = 7;
const LAST_HOUR: u32 = 22;

/// The width of the hours on the left of the week.
const GUTTER: f32 = 52.0;

/// The least height of an event of the week: a short one still readable.
const MIN_EVENT: f32 = 22.0;

/// The height from which an event of the week has room for its details.
const TWO_LINES: f32 = 40.0;

/// The height of an event in a day of the month.
const MONTH_EVENT: f32 = 20.0;

/// The height of a day of the month.
const CELL_HEIGHT: f32 = 112.0;

/// `a` with a part `amount` of `b`: a background lit, opaque.
fn mix(a: iced::Color, b: iced::Color, amount: f32) -> iced::Color {
    iced::Color {
        r: a.r + (b.r - a.r) * amount,
        g: a.g + (b.g - a.g) * amount,
        b: a.b + (b.b - a.b) * amount,
        a: 1.0,
    }
}

/// The minute of the day an event starts at ("…T18:30" is 1110).
pub fn minute_of(start: &str) -> Option<u32> {
    let hour: u32 = start.get(11..13)?.parse().ok()?;
    let minute: u32 = start.get(14..16)?.parse().ok()?;
    Some(hour * 60 + minute)
}

/// The hours the week shows, the last excluded: 7–22, more when an event
/// starts before or ends after.
pub fn hours(days: &[AgendaDay]) -> (u32, u32) {
    days.iter()
        .flat_map(|d| &d.events)
        .filter_map(|e| minute_of(&e.start).map(|start| (start, start + u32::from(e.minutes))))
        .fold((FIRST_HOUR, LAST_HOUR), |(first, last), (start, end)| {
            (first.min(start / 60), last.max(end.div_ceil(60).min(24)))
        })
}

/// Where each event of a day goes across its column: its lane and the
/// lanes of the events it overlaps with, directly or through others
/// (each `1 / lanes` wide). The events come in order of time.
pub fn lanes(events: &[AgendaEventRecord]) -> Vec<(usize, usize)> {
    // an event shorter than half an hour takes half an hour: so it's drawn
    let spans: Vec<(u32, u32)> = events
        .iter()
        .map(|e| {
            let start = minute_of(&e.start).unwrap_or(0);
            (start, start + u32::from(e.minutes).max(30))
        })
        .collect();
    // the groups: the events overlapping each other, one after another
    let groups = spans.iter().enumerate().fold(
        Vec::<(u32, Vec<usize>)>::new(),
        |mut groups, (i, &(start, end))| {
            match groups.last_mut() {
                Some((group_end, members)) if start < *group_end => {
                    *group_end = (*group_end).max(end);
                    members.push(i);
                }
                _ => groups.push((end, vec![i])),
            }
            groups
        },
    );
    let mut placed = vec![(0, 1); spans.len()];
    for (_, members) in groups {
        // each in the first lane free at its start
        let mut ends: Vec<u32> = vec![];
        let lanes: Vec<(usize, usize)> = members
            .iter()
            .filter_map(|&i| spans.get(i).map(|span| (i, *span)))
            .map(|(i, (start, end))| {
                let lane = ends.iter().position(|e| *e <= start).unwrap_or(ends.len());
                match ends.get_mut(lane) {
                    Some(e) => *e = end,
                    None => ends.push(end),
                }
                (i, lane)
            })
            .collect();
        for (i, lane) in lanes {
            if let Some(slot) = placed.get_mut(i) {
                *slot = (lane, ends.len());
            }
        }
    }
    placed
}

/// The seven days of a row of the month, the ones of the months around
/// too.
fn whole_week(week: &[Option<NaiveDate>; 7]) -> [NaiveDate; 7] {
    let monday = week
        .iter()
        .enumerate()
        .find_map(|(i, d)| d.map(|d| d - Duration::days(i as i64)))
        .unwrap_or_default();
    std::array::from_fn(|i| monday + Duration::days(i as i64))
}

/// An event as a box: a bar of its color on the left (a session and a
/// match apart by it and by the icon), on a light background of it; the
/// one of the cursor with a border of the accent.
#[allow(clippy::too_many_arguments)]
fn event_card<'a>(
    event: &AgendaEventRecord,
    s: &'static Strings,
    c: Colors,
    compact: bool,
    with_team: bool,
    chosen: bool,
    message: Option<Message>,
    height: f32,
) -> Element<'a, Message> {
    let color = match event.item {
        AgendaItemRecord::Session { .. } => c.accent,
        AgendaItemRecord::Match { .. } => c.break_point,
    };
    let stripe = container(space::vertical())
        .width(Length::Fixed(3.0))
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(color)),
            ..container::Style::default()
        });
    let body = container(event_text(event, s, c, compact, with_team))
        .padding(Padding::from([2, 6]))
        .width(Length::Fill)
        .clip(true);
    iced::widget::button(row![stripe, body].height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .padding(0)
        .on_press_maybe(message)
        .style(move |_, status| {
            let hovered = matches!(
                status,
                iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed
            );
            iced::widget::button::Style {
                background: Some(Background::Color(mix(
                    c.panel,
                    color,
                    match hovered {
                        true => 0.32,
                        false => 0.2,
                    },
                ))),
                text_color: c.text,
                border: match chosen {
                    true => iced::border::color(c.accent).width(2).rounded(4),
                    false => iced::border::rounded(4),
                },
                ..iced::widget::button::Style::default()
            }
        })
        .into()
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use {
        super::*, crate::strings::strings, scout4all_core::shapes::enums::LanguageEnum,
        scout4all_mobile::venues::VenueDraft,
    };

    fn event(start: &str, minutes: u16) -> AgendaEventRecord {
        AgendaEventRecord {
            team_id: "t".into(),
            team_name: "T".into(),
            start: start.into(),
            minutes,
            item: AgendaItemRecord::Session {
                id: start.into(),
                title: String::new(),
                status: SessionStatus::Planned,
                to_close: false,
            },
            venue: None,
            shared_venue: false,
        }
    }

    /// The week shows 7–22, more for an event before or after; an event
    /// past midnight stops at it.
    #[test]
    fn the_hours_of_the_week_hold_its_events() {
        let day = |events| AgendaDay {
            date: "2026-11-23".into(),
            events,
        };
        assert_eq!(hours(&[day(vec![event("2026-11-23T18:00", 90)])]), (7, 22));
        assert_eq!(
            hours(&[day(vec![
                event("2026-11-23T06:30", 60),
                event("2026-11-23T21:30", 60)
            ])]),
            (6, 23)
        );
        assert_eq!(hours(&[day(vec![event("2026-11-23T23:00", 120)])]), (7, 24));
    }

    /// Events at the same time go side by side, each in the first lane
    /// free; one starting as they end takes the whole column again.
    #[test]
    fn events_at_the_same_time_go_side_by_side() {
        let events = [
            event("2026-11-23T18:00", 90),
            event("2026-11-23T18:30", 60),
            event("2026-11-23T19:30", 60),
            event("2026-11-23T21:00", 60),
        ];
        assert_eq!(lanes(&events), vec![(0, 2), (1, 2), (0, 1), (0, 1)]);
        assert_eq!(minute_of("2026-11-23T18:30"), Some(1110));
    }

    /// A row of the month has all its days, the ones around too.
    #[test]
    fn a_row_of_the_month_has_its_seven_days() {
        let day = |m: u32, d: u32| NaiveDate::from_ymd_opt(2026, m, d).unwrap();
        let first = month_weeks(day(11, 1)).into_iter().next().unwrap();
        assert_eq!(whole_week(&first)[0], day(10, 26));
        assert_eq!(whole_week(&first)[6], day(11, 1));
    }

    fn offers_venues(tab: &CalendarTab) -> bool {
        let s = strings(LanguageEnum::It);
        tab.hints(s).iter().any(|(k, _)| k == "v")
    }

    /// Without gyms there is no filter by gym (a button that did nothing);
    /// with one, `v` goes from all the gyms to it and back; a gym removed
    /// meanwhile filters no more.
    #[test]
    fn the_gym_filter_is_there_only_with_gyms() {
        let dir = tempfile::tempdir().unwrap();
        let core = Scout4All::new(dir.path().to_string_lossy().into_owned(), "it".into()).unwrap();
        let team = core.tutorial_team().unwrap();
        let mut tab = CalendarTab::new(core.clone(), team);
        assert!(!offers_venues(&tab));
        tab.key(key!(v));
        assert!(tab.venue.is_none());
        let gym = core
            .save_venue(
                None,
                VenueDraft {
                    name: "Terontola".into(),
                    ..VenueDraft::default()
                },
            )
            .unwrap()
            .id;
        tab.reload();
        assert!(offers_venues(&tab));
        tab.key(key!(v));
        assert_eq!(tab.venue.as_ref().map(|(id, _)| id), Some(&gym));
        tab.key(key!(v));
        assert!(tab.venue.is_none());
        tab.key(key!(v));
        core.remove_venue(gym).unwrap();
        tab.reload();
        assert!(tab.venue.is_none());
        assert!(!offers_venues(&tab));
    }
}
